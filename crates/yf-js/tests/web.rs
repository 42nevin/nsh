//! Web APIs installed by the host: encoding, URL, headers, abort and fetch.

use yf_core::Op;
use yf_js::{Host, HttpResponse, NetworkQueue};

/// Runs `script` (which sets `globalThis.out`) and returns `out` as a string.
fn run(host: &Host, script: &str) -> String {
    host.eval_source(&format!(
        "{script}\n;__yf_commit([1, 1, typeof out === 'string' ? out : JSON.stringify(out)]);"
    ))
    .unwrap();
    output(host)
}

fn output(host: &Host) -> String {
    let mut ops = Vec::new();
    host.take_ops(&mut ops);
    match ops.pop() {
        Some(Op::CreateText { text, .. }) => text.to_string(),
        other => panic!("no output: {other:?}"),
    }
}

#[test]
fn lazy_globals_resolve_and_can_be_replaced() {
    let host = Host::new().unwrap();
    let out = run(
        &host,
        r#"
        globalThis.fetch = "mine";
        const names = ["TextEncoder", "TextDecoder", "URL", "URLSearchParams", "Event", "EventTarget",
            "AbortSignal", "AbortController", "Headers", "Request", "Response", "fetch"];
        globalThis.out = names.map((n) => typeof globalThis[n]).join(",") + " " + Object.keys(globalThis).includes("URL");
        "#,
    );
    assert_eq!(
        out,
        "function,function,function,function,function,function,function,function,function,function,function,string false"
    );
}

#[test]
fn text_encoding_round_trips() {
    let host = Host::new().unwrap();
    let out = run(
        &host,
        r#"
        const bytes = new TextEncoder().encode("héllo 👋");
        const sub = new Uint8Array([0, ...bytes]).subarray(1);
        globalThis.out = [bytes.length, new TextDecoder().decode(sub), new TextDecoder().decode(new Uint8Array([0xff]))];
        try { new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array([0xff])); out.push("no throw"); }
        catch (e) { out.push(e instanceof TypeError); }
        "#,
    );
    assert_eq!(out, r#"[11,"héllo 👋","�",true]"#);
}

#[test]
fn urls_follow_whatwg() {
    let host = Host::new().unwrap();
    let out = run(
        &host,
        r#"
        const u = new URL("../b?x=1&y=a+b#h", "https://user:pw@例え.jp:8443/a/c");
        u.searchParams.append("z", "c d&e");
        const p = new URLSearchParams({ q: "1 2", r: "~!" });
        globalThis.out = [u.href, u.origin, u.hostname, u.port, u.searchParams.get("y"), p.toString(), URL.canParse("nope"), String(new URL("HTTP://X.COM"))];
        u.pathname = "/new path";
        out.push(u.pathname);
        "#,
    );
    assert_eq!(
        out,
        r#"["https://user:pw@xn--r8jz45g.jp:8443/b?x=1&y=a+b&z=c+d%26e#h","https://xn--r8jz45g.jp:8443","xn--r8jz45g.jp","8443","a b","q=1+2&r=%7E%21",false,"http://x.com/","/new%20path"]"#
    );
}

#[test]
fn headers_combine_and_sort() {
    let host = Host::new().unwrap();
    let out = run(
        &host,
        r#"
        const h = new Headers({ "X-B": "2", Accept: "text/html" });
        h.append("x-b", " 3 ");
        h.append("Set-Cookie", "a=1");
        h.append("Set-Cookie", "b=2");
        globalThis.out = [h.get("X-b"), [...h].map(([k, v]) => k + "=" + v).join(";"), h.has("accept")];
        try { h.set("bad name", "x"); } catch (e) { out.push(e instanceof TypeError); }
        "#,
    );
    assert_eq!(
        out,
        r#"["2, 3","accept=text/html;set-cookie=a=1;set-cookie=b=2;x-b=2, 3",true,true]"#
    );
}

#[test]
fn fetch_goes_through_the_network_queue() {
    let host = Host::new().unwrap();
    host.eval_source(
        r#"
        globalThis.out = [];
        fetch("https://example.com/api?q=1", {
            method: "post",
            headers: { "X-Token": "t" },
            body: JSON.stringify({ a: 1 }),
        }).then(async (r) => {
            out.push(r.status, r.ok, r.headers.get("content-type"), r.url, await r.json());
            try { await r.text(); } catch (e) { out.push("used"); }
        });
        const c = new AbortController();
        fetch("https://example.com/slow", { signal: c.signal }).catch((e) => out.push(e.name));
        c.abort();
        fetch("https://example.com/down").catch((e) => out.push(e instanceof TypeError, e.message));
        "#,
    )
    .unwrap();
    let mut queue = NetworkQueue::default();
    host.take_network(&mut queue);
    // The aborted request never reached the network.
    assert_eq!(queue.requests.len(), 2);
    assert!(queue.cancels.is_empty());
    let (id, req) = &queue.requests[0];
    assert_eq!(req.method, "POST");
    assert_eq!(req.url, "https://example.com/api?q=1");
    assert_eq!(req.body, br#"{"a":1}"#);
    assert!(req.headers.contains(&("x-token".into(), "t".into())));
    assert!(
        req.headers
            .contains(&("content-type".into(), "text/plain;charset=UTF-8".into()))
    );
    host.finish_fetch(
        *id,
        Ok(HttpResponse {
            status: 201,
            status_text: "Created".into(),
            url: "https://example.com/api?q=1".into(),
            redirected: false,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: br#"{"ok":true}"#.to_vec(),
        }),
    );
    host.finish_fetch(queue.requests[1].0, Err("offline".into()));
    host.eval_source("__yf_commit([1, 1, JSON.stringify(out)]);")
        .unwrap();
    assert_eq!(
        output(&host),
        r#"["AbortError",201,true,"application/json","https://example.com/api?q=1",{"ok":true},"used",true,"Network request failed: offline"]"#
    );
}

#[test]
fn aborting_an_inflight_fetch_cancels_it() {
    let host = Host::new().unwrap();
    host.eval_source(
        r#"
        globalThis.c = new AbortController();
        globalThis.out = [];
        fetch("https://example.com/", { signal: AbortSignal.any([c.signal]) }).catch((e) => out.push(e.name));
        "#,
    )
    .unwrap();
    let mut queue = NetworkQueue::default();
    host.take_network(&mut queue);
    let id = queue.requests[0].0;
    host.eval_source("c.abort();").unwrap();
    let mut queue = NetworkQueue::default();
    host.take_network(&mut queue);
    assert_eq!(queue.cancels, vec![id]);
    // A late completion is ignored.
    host.finish_fetch(id, Err("cancelled".into()));
    host.eval_source("__yf_commit([1, 1, JSON.stringify(out)]);")
        .unwrap();
    assert_eq!(output(&host), r#"["AbortError"]"#);
}

#[test]
#[ignore]
fn bench_host_startup() {
    let _ = Host::new().unwrap();
    let n = 50;
    let start = std::time::Instant::now();
    for _ in 0..n {
        drop(Host::new().unwrap());
    }
    eprintln!("Host::new: {:?}", start.elapsed() / n);
}
