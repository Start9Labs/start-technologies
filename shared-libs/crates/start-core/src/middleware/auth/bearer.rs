use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http::header::AUTHORIZATION;
use rpc_toolkit::{Middleware, RpcRequest, RpcResponse};
use serde::Deserialize;

use crate::auth::{LoginContext, touch_token};
use crate::prelude::*;

const TOKEN_PREFIX: &str = "startos_";

/// A new bearer token: the secret to hand the client once, and the session id
/// it authenticates as.
pub fn new_token() -> (String, InternedString) {
    let secret = format!(
        "{TOKEN_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
    );
    let id = token_session_id(&secret);
    (secret, id)
}

/// The id a bearer token is stored under; the secret itself is never stored.
pub fn token_session_id(secret: &str) -> InternedString {
    InternedString::intern(URL_SAFE_NO_PAD.encode(blake3::hash(secret.as_bytes()).as_bytes()))
}

#[derive(Deserialize)]
pub struct Metadata {
    #[serde(default)]
    get_signer: bool,
}

/// Authenticates a request whose `Authorization: Bearer` header carries a
/// token from `auth.token.create`, as the session that token was created as.
#[derive(Clone)]
pub struct BearerAuth {
    session: Option<InternedString>,
}
impl BearerAuth {
    pub fn new() -> Self {
        Self { session: None }
    }
}
impl<C: LoginContext> Middleware<C> for BearerAuth {
    type Metadata = Metadata;
    async fn process_http_request(
        &mut self,
        _: &C,
        request: &mut axum::extract::Request,
    ) -> Result<(), axum::response::Response> {
        self.session = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .filter(|token| token.starts_with(TOKEN_PREFIX))
            .map(token_session_id);
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        context: &C,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        async {
            let session = self.session.clone().ok_or_else(|| {
                Error::new(
                    eyre!("{}", t!("middleware.auth.unauthorized")),
                    ErrorKind::Authorization,
                )
            })?;
            let enrolled = C::access_tokens(&mut context.db().peek().await)
                .is_some_and(|tokens| tokens.as_idx(&session).is_some());
            if !enrolled {
                return Err(Error::new(
                    eyre!("{}", t!("middleware.auth.key-not-authorized")),
                    ErrorKind::Authorization,
                ));
            }
            if metadata.get_signer {
                request.params["__Auth_signer"] = to_value(&session)?;
            }
            touch_token(context, &session).await
        }
        .await
        .map_err(|e: Error| RpcResponse::from_result(Err(e)))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use axum::body::Body;
    use http_body_util::BodyExt;
    use rpc_toolkit::{Context, HandlerExt, HttpServer, ParentHandler, Server, from_fn_async};
    use serde::Serialize;

    use super::*;
    use crate::auth::{
        AuthKeys, CreateTokenParams, Sessions, TokenSessions, create_token, revoke_tokens,
    };
    use crate::middleware::auth::signature::{
        AUTH_SIG_HEADER, LoginMetadata, NonceCache, SignatureAuthContext, SignatureHeader,
        check_enrolled,
    };
    use crate::middleware::auth::{Auth, DbContext};
    use crate::rpc_continuations::OpenAuthedContinuations;
    use crate::sign::{AnySigningKey, AnyVerifyingKey};
    use crate::util::sync::SyncMutex;

    #[derive(Default, Deserialize, Serialize, HasModel)]
    #[serde(rename_all = "camelCase")]
    #[model = "Model<Self>"]
    struct TestDb {
        sessions: Sessions,
    }

    #[derive(Clone)]
    struct TestCtx(Arc<TestState>);
    struct TestState {
        db: TypedPatchDb<TestDb>,
        nonces: SyncMutex<NonceCache>,
        continuations: OpenAuthedContinuations<Option<InternedString>>,
    }
    impl Context for TestCtx {}
    impl DbContext for TestCtx {
        type Database = TestDb;
        fn db(&self) -> &TypedPatchDb<TestDb> {
            &self.0.db
        }
    }
    impl SignatureAuthContext for TestCtx {
        type AdditionalMetadata = LoginMetadata;
        type CheckPubkeyRes = ();
        fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T {
            self.0.nonces.mutate(f)
        }
        async fn clock_synced(&self) -> bool {
            false
        }
        fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
            &self.0.continuations
        }
        fn remove_enrolled_keys(
            db: &mut Model<TestDb>,
            keys: &BTreeSet<InternedString>,
        ) -> Result<(), Error> {
            for key in keys {
                db.as_sessions_mut().as_pubkeys_mut().remove(key)?;
            }
            Ok(())
        }
        async fn sig_context(
            &self,
            _: &http::Extensions,
        ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send {
            [Ok::<_, Error>("localhost")]
        }
        fn check_pubkey(
            &self,
            db: &Model<TestDb>,
            pubkey: Option<&AnyVerifyingKey>,
            _: LoginMetadata,
        ) -> Result<(), Error> {
            check_enrolled(pubkey, false, |id| {
                Ok(db.as_sessions().as_pubkeys().as_idx(id).is_some())
            })
            .map(|_| ())
        }
        async fn post_auth_hook(&self, _: (), _: &RpcRequest) -> Result<(), Error> {
            Ok(())
        }
    }
    impl LoginContext for TestCtx {
        fn access_auth_keys(db: &mut Model<TestDb>) -> &mut Model<AuthKeys> {
            db.as_sessions_mut().as_pubkeys_mut()
        }
        fn access_tokens(db: &mut Model<TestDb>) -> Option<&mut Model<TokenSessions>> {
            Some(db.as_sessions_mut().as_tokens_mut())
        }
        fn check_password(_: &Model<TestDb>, _: &str) -> Result<(), Error> {
            Ok(())
        }
    }

    #[derive(Deserialize, Serialize)]
    struct WhoAmI {
        #[serde(rename = "__Auth_signer")]
        signer: Option<InternedString>,
    }

    async fn context() -> TestCtx {
        TestCtx(Arc::new(TestState {
            db: TypedPatchDb::load_or_init(patch_db::PatchDb::in_memory(), || async {
                Ok(TestDb::default())
            })
            .await
            .unwrap(),
            nonces: SyncMutex::new(NonceCache::default()),
            continuations: OpenAuthedContinuations::new(),
        }))
    }

    fn server(ctx: &TestCtx) -> HttpServer<TestCtx> {
        let ctx = ctx.clone();
        Server::new(
            move || {
                let ctx = ctx.clone();
                async move { Ok(ctx) }
            },
            ParentHandler::new().subcommand(
                "whoami",
                from_fn_async(|_: TestCtx, p: WhoAmI| async move { Ok::<_, Error>(p.signer) })
                    .with_metadata("get_signer", Value::Bool(true))
                    .no_cli()
                    .no_ts(),
            ),
        )
        .middleware(Auth::new().with_bearer_auth().with_signature_auth())
    }

    const WHOAMI_REQUEST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"whoami","params":{}}"#;

    async fn whoami(server: &HttpServer<TestCtx>, headers: http::HeaderMap) -> serde_json::Value {
        let mut request = http::Request::builder()
            .method("POST")
            .uri("/rpc/v1")
            .body(Body::from(WHOAMI_REQUEST))
            .unwrap();
        *request.headers_mut() = headers;
        let response = server.handle(request).await;
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }

    fn bearer_headers(token: &str) -> http::HeaderMap {
        http::HeaderMap::from_iter([(AUTHORIZATION, format!("Bearer {token}").parse().unwrap())])
    }

    #[tokio::test]
    async fn signature_replays_keep_their_error_code() {
        let ctx = context().await;
        let server = server(&ctx);
        let key = AnySigningKey::Ed25519(ed25519_dalek::SigningKey::generate(
            &mut crate::util::crypto::os_rng(),
        ));
        let id = key.verifying_key().interned_der64();
        let now = chrono::Utc::now();
        ctx.db()
            .mutate(|db| {
                db.as_sessions_mut().as_pubkeys_mut().insert(
                    &id,
                    &crate::auth::Session {
                        logged_in: now,
                        last_active: now,
                        ..Default::default()
                    },
                )
            })
            .await
            .result
            .unwrap();
        let signature = SignatureHeader::sign(&key.into(), WHOAMI_REQUEST.as_bytes(), "localhost")
            .await
            .unwrap()
            .to_header();
        let mut headers = http::HeaderMap::new();
        headers.insert(AUTH_SIG_HEADER, signature);
        let response = whoami(&server, headers.clone()).await;
        assert_eq!(response["result"], &*id, "{response}");
        let replay = whoami(&server, headers).await;
        assert_eq!(
            replay["error"]["code"],
            ErrorKind::InvalidSignature as i32,
            "{replay}"
        );
    }

    async fn create(ctx: &TestCtx, name: &str) -> crate::auth::CreatedToken {
        create_token(
            ctx.clone(),
            CreateTokenParams {
                name: InternedString::intern(name),
            },
        )
        .await
        .unwrap()
    }

    #[test]
    fn session_ids_hash_the_secret() {
        let (secret, id) = new_token();
        assert!(secret.starts_with(TOKEN_PREFIX));
        assert_eq!(id, token_session_id(&secret));
        assert!(!id.contains(&secret[TOKEN_PREFIX.len()..]));
        assert_ne!(new_token().1, id);
    }

    #[tokio::test]
    async fn tokens_authenticate_as_their_session_until_revoked() {
        let ctx = context().await;
        let server = server(&ctx);
        let created = create(&ctx, "claude").await;
        let sessions = ctx.db().peek().await.into_sessions().de().unwrap();
        assert!(sessions.pubkeys.0.is_empty());
        let session = &sessions.tokens[&created.id];
        assert_eq!(session.name.as_deref(), Some("claude"));
        assert!(
            !serde_json::to_string(&sessions)
                .unwrap()
                .contains(&created.token)
        );

        let before = session.last_active;
        let response = whoami(&server, bearer_headers(&created.token)).await;
        assert_eq!(response["result"], &*created.id, "{response}");
        let tokens = ctx
            .db()
            .peek()
            .await
            .into_sessions()
            .into_tokens()
            .de()
            .unwrap();
        assert!(tokens[&created.id].last_active >= before);

        let continuations = &ctx.0.continuations;
        ctx.db()
            .mutate(|db| revoke_tokens::<TestCtx>(continuations, db, [created.id.clone()]))
            .await
            .result
            .unwrap();
        let response = whoami(&server, bearer_headers(&created.token)).await;
        assert!(response["error"].is_object(), "{response}");
    }

    #[tokio::test]
    async fn other_bearers_are_rejected() {
        let ctx = context().await;
        let server = server(&ctx);
        create(&ctx, "claude").await;
        let forged = new_token().0;
        let cookie = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
        for bearer in [None, Some(forged.as_str()), Some(cookie.as_str())] {
            let response = whoami(&server, bearer.map(bearer_headers).unwrap_or_default()).await;
            assert!(response["error"].is_object(), "{bearer:?}: {response}");
        }
    }
}
