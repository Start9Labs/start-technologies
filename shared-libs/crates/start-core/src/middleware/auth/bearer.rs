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
            let enrolled = context.auth_token_is_enrolled(&context.db().peek().await, &session);
            if !enrolled {
                return Err(Error::new(
                    eyre!("{}", t!("middleware.auth.key-not-authorized")),
                    ErrorKind::Authorization,
                ));
            }
            if metadata.get_signer {
                crate::middleware::params_object(&mut request.params)?
                    .insert("__Auth_signer".into(), to_value(&session)?);
            }
            touch_token(context, &session).await
        }
        .await
        .map_err(|e: Error| RpcResponse::from_result(Err(e)))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use axum::body::Body;
    use http_body_util::BodyExt;
    use rpc_toolkit::{Context, HandlerExt, HttpServer, ParentHandler, Server, from_fn_async};
    use serde::Serialize;

    use super::*;
    use crate::auth::{
        AuthKeys, CreateTokenParams, LoginAttempt, Sessions, TokenSessions, create_token,
        revoke_tokens,
    };
    use crate::middleware::auth::signature::{
        AUTH_SIG_HEADER, LoginMetadata, NonceCache, SignatureAuthContext,
        SignatureAuthorizationContext, SignatureHeader, check_enrolled,
    };
    use crate::middleware::auth::{Auth, DbContext};
    use crate::rpc_continuations::OpenAuthedContinuations;
    use crate::sign::{AnySigningKey, AnyVerifyingKey};
    use crate::util::sync::SyncMutex;

    #[derive(Default, Deserialize, Serialize, HasModel)]
    #[serde(rename_all = "camelCase")]
    #[model = "Model<Self>"]
    pub(crate) struct TestDb {
        sessions: Sessions,
    }

    #[derive(Clone)]
    pub(crate) struct TestCtx(Arc<TestState>);
    struct TestState {
        db: TypedPatchDb<TestDb>,
        nonces: SyncMutex<NonceCache>,
        continuations: OpenAuthedContinuations<Option<InternedString>>,
        ephemeral: SyncMutex<AuthKeys>,
        login_attempts: SyncMutex<Vec<(LoginAttempt, bool)>>,
    }
    impl Context for TestCtx {}
    impl DbContext for TestCtx {
        type Database = TestDb;
        fn db(&self) -> &TypedPatchDb<TestDb> {
            &self.0.db
        }
    }
    impl crate::middleware::auth::session::SessionContext for TestCtx {}

    impl SignatureAuthContext for TestCtx {
        fn server_identity(&self) -> Result<crate::sign::Signer, Error> {
            panic!("HTTP-only fixture")
        }
        fn replay_check(&self) -> super::super::signature::RequestReplayCheck {
            super::super::signature::RequestReplayCheck::new(&self.0, |state| Some(Self(state)))
        }
        fn with_secure_session(self, _: crate::middleware::auth::session::SecureSession) -> Self {
            panic!("HTTP-only fixture")
        }
        fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T {
            self.0.nonces.mutate(f)
        }
        async fn clock_synced(&self) -> bool {
            false
        }
        async fn sig_context(
            &self,
            _: &http::Extensions,
        ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send {
            [Ok::<_, Error>("localhost")]
        }
    }
    impl SignatureAuthorizationContext for TestCtx {
        type AdditionalMetadata = LoginMetadata;
        type CheckPubkeyRes = ();
        fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
            &self.0.continuations
        }
        fn ephemeral_auth_keys(&self) -> Option<&SyncMutex<AuthKeys>> {
            Some(&self.0.ephemeral)
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
        fn check_pubkey(
            &self,
            db: &Model<TestDb>,
            pubkey: Option<&AnyVerifyingKey>,
            metadata: LoginMetadata,
        ) -> Result<(), Error> {
            check_enrolled(pubkey, metadata.login, |id| {
                self.auth_key_is_enrolled(db, id)
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
        fn check_password(_: &Model<TestDb>, password: &str) -> Result<(), Error> {
            if password == WRONG_PASSWORD {
                return Err(Error::new(eyre!("wrong"), ErrorKind::IncorrectPassword));
            }
            Ok(())
        }
        async fn login_attempt_hook(&self, attempt: &LoginAttempt, result: Result<(), &Error>) {
            self.0
                .login_attempts
                .mutate(|attempts| attempts.push((attempt.clone(), result.is_ok())));
        }
    }

    const WRONG_PASSWORD: &str = "wrong-password";

    #[derive(Deserialize, Serialize)]
    struct WhoAmI {
        #[serde(rename = "__Auth_signer")]
        signer: Option<InternedString>,
    }

    pub(crate) async fn context() -> TestCtx {
        TestCtx(Arc::new(TestState {
            db: TypedPatchDb::load_or_init(patch_db::PatchDb::in_memory(), || async {
                Ok(TestDb::default())
            })
            .await
            .unwrap(),
            nonces: SyncMutex::new(NonceCache::default()),
            continuations: OpenAuthedContinuations::new(),
            ephemeral: SyncMutex::new(AuthKeys::default()),
            login_attempts: SyncMutex::new(Vec::new()),
        }))
    }

    fn server(ctx: &TestCtx) -> HttpServer<TestCtx> {
        let ctx = ctx.clone();
        Server::new(
            move || {
                let ctx = ctx.clone();
                async move { Ok(ctx) }
            },
            ParentHandler::new()
                .subcommand(
                    "whoami",
                    from_fn_async(|_: TestCtx, p: WhoAmI| async move { Ok::<_, Error>(p.signer) })
                        .with_metadata("get_signer", Value::Bool(true))
                        .no_cli()
                        .no_ts(),
                )
                .subcommand(
                    "login",
                    from_fn_async(crate::auth::login_impl::<TestCtx>)
                        .with_metadata("login", Value::Bool(true))
                        .with_metadata("get_signer", Value::Bool(true))
                        .with_metadata("get_user_agent", Value::Bool(true))
                        .with_metadata("get_peer", Value::Bool(true))
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

    async fn login(
        server: &HttpServer<TestCtx>,
        key: &AnySigningKey,
        password: &str,
    ) -> serde_json::Value {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "login",
            "params": { "password": password, "pubkey": key.verifying_key() },
        })
        .to_string();
        let signature = SignatureHeader::sign(&key.clone().into(), body.as_bytes(), "localhost")
            .await
            .unwrap()
            .to_header();
        let mut request = http::Request::builder()
            .method("POST")
            .uri("/rpc/v1")
            .header(AUTH_SIG_HEADER, signature)
            .header(http::header::USER_AGENT, "test-client/1")
            .body(Body::from(body))
            .unwrap();
        request
            .extensions_mut()
            .insert(crate::net::web_server::TcpMetadata {
                peer_addr: "192.168.0.23:51234".parse().unwrap(),
                local_addr: "192.168.0.1:443".parse().unwrap(),
            });
        let response = server.handle(request).await;
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }

    fn bearer_headers(token: &str) -> http::HeaderMap {
        http::HeaderMap::from_iter([(AUTHORIZATION, format!("Bearer {token}").parse().unwrap())])
    }

    #[tokio::test]
    async fn login_attempts_carry_their_peer_and_outcome() {
        let ctx = context().await;
        let server = server(&ctx);
        let key = AnySigningKey::Ed25519(ed25519_dalek::SigningKey::generate(
            &mut crate::util::crypto::os_rng(),
        ));

        let accepted = login(&server, &key, "right-password").await;
        assert!(accepted["error"].is_null(), "{accepted}");
        let refused = login(&server, &key, WRONG_PASSWORD).await;
        assert_eq!(
            refused["error"]["code"],
            ErrorKind::IncorrectPassword as i32,
            "{refused}"
        );

        let peer: Option<std::net::IpAddr> = Some("192.168.0.23".parse().unwrap());
        let agent = Some("test-client/1".to_string());
        let attempts = ctx.0.login_attempts.peek(|attempts| {
            attempts
                .iter()
                .map(|(attempt, ok)| (attempt.peer, attempt.user_agent.clone(), *ok))
                .collect::<Vec<_>>()
        });
        assert_eq!(
            attempts,
            [(peer, agent.clone(), true), (peer, agent, false)]
        );
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
    async fn token_continuations_recheck_membership_and_keep_unbound_local_scopes() {
        use futures::FutureExt;
        let ctx = context().await;
        let token = create(&ctx, "continuation").await;
        let mut bound = crate::auth::continuation_subscription(&ctx, Some(token.id.clone()))
            .await
            .unwrap();
        let mut local = crate::auth::continuation_subscription(&ctx, None)
            .await
            .unwrap();
        assert!(futures::poll!(&mut bound).is_pending());
        let continuations = &ctx.0.continuations;
        ctx.db()
            .mutate(|db| revoke_tokens::<TestCtx>(continuations, db, [token.id.clone()]))
            .await
            .result
            .unwrap();
        assert!(bound.now_or_never().is_some());
        let Err(error) = crate::auth::continuation_subscription(&ctx, Some(token.id)).await else {
            panic!("revoked token bound a continuation")
        };
        assert_eq!(error.kind, ErrorKind::Authorization);
        assert!(futures::poll!(&mut local).is_pending());
    }

    #[tokio::test]
    async fn ephemeral_key_continuations_use_the_live_key_owner() {
        use futures::FutureExt;
        let ctx = context().await;
        let key = AnySigningKey::generate().verifying_key().interned_der64();
        let now = chrono::Utc::now();
        ctx.0.ephemeral.mutate(|keys| {
            keys.0.insert(
                key.clone(),
                crate::auth::Session {
                    name: None,
                    logged_in: now,
                    last_active: now,
                    user_agent: None,
                },
            );
        });
        let bound = crate::auth::continuation_subscription(&ctx, Some(key.clone()))
            .await
            .unwrap();
        assert!(
            ctx.db()
                .peek()
                .await
                .as_sessions()
                .as_pubkeys()
                .de()
                .unwrap()
                .0
                .is_empty()
        );
        ctx.unenroll([key.clone()]).await.unwrap();
        assert!(bound.now_or_never().is_some());
        let Err(error) = crate::auth::continuation_subscription(&ctx, Some(key)).await else {
            panic!("revoked ephemeral key bound a continuation")
        };
        assert_eq!(error.kind, ErrorKind::Authorization);
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
