use super::*;
use crate::disk::mount::util::is_mountpoint;
use crate::util::Invoke;

async fn node_rootfs(root: &Path) {
    let node = tokio::process::Command::new("node")
        .args(["--print", "process.execPath"])
        .invoke(crate::ErrorKind::Filesystem)
        .await
        .unwrap();
    let node = String::from_utf8(node).unwrap();
    let node = node.trim();
    let libraries = tokio::process::Command::new("ldd")
        .arg(node)
        .invoke(crate::ErrorKind::Filesystem)
        .await
        .unwrap();
    let libraries = String::from_utf8(libraries).unwrap();
    for (source, destination) in std::iter::once((node, "/usr/bin/node")).chain(
        libraries
            .split_whitespace()
            .filter(|path| path.starts_with('/'))
            .map(|path| (path, path)),
    ) {
        let destination = root.join(destination.trim_start_matches('/'));
        tokio::fs::create_dir_all(destination.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::copy(source, destination).await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires root, private mount namespace, Node.js and util-linux"]
async fn provider_mounts_isolate_continuations_in_outer_and_subcontainer_roots() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let sockets = sockets().await;
        let provider = "waf".parse().unwrap();
        let other = sockets
            .prepare_provider(&"anubis".parse().unwrap())
            .await
            .unwrap();
        let (target, _app) = echo_application().await;
        let _route = ChainRoute::new(&sockets, &binding(), &[stage("waf")], target).await;
        for _ in 0..2 {
            let rootfs = TmpDir::new().await.unwrap();
            node_rootfs(&rootfs).await;
            let mounts = sockets.mount_provider(&provider, &rootfs).await.unwrap();
            let mounted = rootfs.join(CONTAINER_SOCKET_DIR.trim_start_matches('/'));
            for (path, expected) in [(&mounted, "ro"), (&mounted.join("in"), "rw")] {
                let options = tokio::process::Command::new("findmnt")
                    .args(["--noheadings", "--output", "VFS-OPTIONS", "--target"])
                    .arg(path)
                    .invoke(crate::ErrorKind::Filesystem)
                    .await
                    .unwrap();
                assert!(
                    String::from_utf8(options)
                        .unwrap()
                        .trim()
                        .split(',')
                        .any(|option| option == expected)
                );
            }
            let ingress = binding().socket_path(Path::new(CONTAINER_SOCKET_DIR), "in", "standard");
            let next = binding().socket_path(Path::new(CONTAINER_SOCKET_DIR), "next", "standard");
            tokio::process::Command::new("chroot")
                .arg("--userspec=100000:100000")
                .arg(&*rootfs)
                .args(["/usr/bin/node", "-e", PROVIDER_PROBE])
                .arg(ingress)
                .arg(next)
                .arg(other.clone())
                .kill_on_drop(true)
                .invoke(crate::ErrorKind::Filesystem)
                .await
                .unwrap();
            for mount in mounts.into_iter().rev() {
                mount.unmount(false).await.unwrap();
            }
            assert!(!is_mountpoint(&mounted).await.unwrap());
            assert!(!is_mountpoint(mounted.join("in")).await.unwrap());
        }
    })
    .await
    .expect("provider mount probe stalled");
}

const PROVIDER_PROBE: &str = r#"
const assert = require('node:assert/strict')
const fs = require('node:fs')
const net = require('node:net')
const path = require('node:path')
const [ingress, next, other] = process.argv.slice(1)
assert.equal(process.getuid(), 100000)
assert.equal(fs.statSync(path.dirname(ingress)).uid, 100000)
assert.equal(fs.statSync(path.dirname(next)).uid, 0)
assert.equal(fs.existsSync(other), false)
assert.deepEqual(fs.readdirSync('/media/startos/http-proxy').sort(), ['in', 'next'])
const denied = operation => assert.throws(operation, error => ['EACCES', 'EROFS'].includes(error.code))
denied(() => fs.unlinkSync(next))
denied(() => fs.writeFileSync(path.join(path.dirname(next), 'forged'), 'replacement'))
denied(() => fs.mkdirSync('/media/startos/http-proxy/other'))
const impostor = net.createServer()
impostor.on('error', error => {
  assert(['EADDRINUSE', 'EACCES', 'EROFS'].includes(error.code))
})
impostor.listen(next, () => { throw new Error('continuation replaced') })
const listener = net.createServer()
listener.listen(ingress, () => {
  const client = net.connect(next)
  let response = ''
  client.on('error', error => { throw error })
  client.on('data', chunk => { response += chunk })
  client.on('end', () => {
    assert(response.includes('200 OK'))
    assert(response.includes('proof=accepted; HttpOnly'))
    assert(response.endsWith('streamed upload'))
    listener.close()
  })
  client.write('POST /upload?x=1 HTTP/1.1\r\nHost: app.example\r\nX-Forwarded-For: 203.0.113.9\r\nContent-Length: 15\r\nConnection: close\r\n\r\nstreamed upload')
})
setTimeout(() => { throw new Error('socket probe stalled') }, 5000).unref()
"#;
