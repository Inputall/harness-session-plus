use hsp_core::{cdp::Error, discovery::discover};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
#[tokio::test]
async fn discovers_version_and_target_over_loopback_http() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        for (path, body) in [
            (
                "/json/version",
                r#"{"Browser":"Mock/1","Protocol-Version":"1.3"}"#,
            ),
            (
                "/json/list",
                r#"[{"id":"test","type":"page","url":"dsh-app://app/","webSocketDebuggerUrl":"ws://127.0.0.1:9223/devtools/page/test"}]"#,
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let len = stream.read(&mut bytes).await.unwrap();
            assert!(len > 0);
            assert!(String::from_utf8_lossy(&bytes[..len]).starts_with(&format!("GET {path} ")));
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
    });
    let (version, targets) = discover(port).await.unwrap();
    assert_eq!(version.browser, "Mock/1");
    assert_eq!(targets[0].id, "test");
    task.await.unwrap();
}
#[tokio::test]
async fn discovery_refuses_redirect_to_other_hosts() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        let len = stream.read(&mut bytes).await.unwrap();
        assert!(len > 0);
        stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://example.org/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    });
    assert_eq!(discover(port).await.unwrap_err(), Error::Discovery);
    task.await.unwrap();
}
