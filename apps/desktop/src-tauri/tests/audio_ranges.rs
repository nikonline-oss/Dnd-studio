#![allow(clippy::unwrap_used)]
use dnd_desktop_lib::assets_api::with_byte_range;
use tauri::http::Response;

#[test]
fn audio_requests_can_seek_in_cached_bytes() {
    for (range, status, bytes, content) in [
        ("bytes=3-5", 206, vec![3, 4, 5], "bytes 3-5/10"),
        ("bytes=7-", 206, vec![7, 8, 9], "bytes 7-9/10"),
        ("bytes=-2", 206, vec![8, 9], "bytes 8-9/10"),
        ("bytes=100-", 416, vec![], "bytes */10"),
    ] {
        let result = with_byte_range(Response::new((0..10).collect()), Some(range));
        assert_eq!(result.status(), status);
        assert_eq!(result.body(), &bytes);
        assert_eq!(result.headers()["content-range"], content);
    }
    let unchanged = with_byte_range(Response::new(vec![1, 2]), None);
    assert_eq!(unchanged.status(), 200);
    assert_eq!(unchanged.headers()["accept-ranges"], "bytes");
    assert_eq!(unchanged.headers()["content-length"], "2");
    let missing = with_byte_range(
        Response::builder().status(404).body(Vec::new()).unwrap(),
        Some("bytes=0-"),
    );
    assert_eq!(missing.status(), 404);
    let empty = with_byte_range(Response::new(Vec::new()), Some("bytes=0-"));
    assert_eq!(empty.status(), 416);
    for range in ["items=0-1", "bytes=0-1,3-4"] {
        assert_eq!(
            with_byte_range(Response::new(vec![1, 2]), Some(range)).status(),
            200
        );
    }
}
