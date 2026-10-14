//! Natives behind the web APIs of `web.js` (encoding, URL parsing, fetch).

use std::{cell::Cell, rc::Rc};

use rquickjs::{
    ArrayBuffer, Ctx, Exception, Function, Module, Value,
    function::Opt,
    object::{Accessor, Property},
};

use crate::{
    host::Shared,
    net::{HttpRequest, HttpResponse, RequestId},
};

/// `web.js`, compiled by the build script.
const BYTECODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/web.qbc"));

/// Installs the natives and arranges for `web.js`, which wraps and hides
/// them, to run on first use.
pub(crate) fn install<'js>(ctx: &Ctx<'js>, shared: &Rc<Shared>) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    globals.set("__yf_utf8_encode", Function::new(ctx.clone(), utf8_encode)?)?;
    globals.set("__yf_utf8_decode", Function::new(ctx.clone(), utf8_decode)?)?;
    globals.set("__yf_url", Function::new(ctx.clone(), url)?)?;

    let s = shared.clone();
    globals.set(
        "__yf_fetch",
        Function::new(
            ctx.clone(),
            move |id: RequestId,
                  method: String,
                  url: String,
                  headers: Vec<String>,
                  body: Option<ArrayBuffer<'_>>,
                  follow_redirects: bool| {
                let headers = headers
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|[name, value]| (name.clone(), value.clone()))
                    .collect();
                // SAFETY: the bytes are copied before any JS runs.
                let body = body
                    .and_then(|b| unsafe { b.as_bytes() }.map(<[u8]>::to_vec))
                    .unwrap_or_default();
                s.net.borrow_mut().requests.push((
                    id,
                    HttpRequest {
                        method,
                        url,
                        headers,
                        body,
                        follow_redirects,
                    },
                ));
            },
        )?,
    )?;
    let s = shared.clone();
    globals.set(
        "__yf_fetch_abort",
        Function::new(ctx.clone(), move |id: RequestId| {
            let mut net = s.net.borrow_mut();
            // Not handed to the network yet: drop it here.
            let before = net.requests.len();
            net.requests.retain(|(r, _)| *r != id);
            if net.requests.len() == before {
                net.cancels.push(id);
            }
        })?,
    )?;
    // Creating the classes costs about as much as the rest of the host
    // setup: do it when an app first touches one of the globals, through
    // accessors that `web.js` replaces with plain values.
    let state = Rc::new(Cell::new(Load::Pending));
    for name in GLOBALS {
        let st = state.clone();
        let get = move |ctx: Ctx<'js>| -> rquickjs::Result<Value<'js>> {
            match st.get() {
                Load::Pending => {
                    st.set(Load::Done);
                    if let Err(err) = load(&ctx) {
                        st.set(Load::Failed);
                        return Err(err);
                    }
                }
                // Never re-enter the accessor.
                Load::Failed => return Ok(Value::new_undefined(ctx)),
                Load::Done => {}
            }
            ctx.globals().get(name)
        };
        let set = move |ctx: Ctx<'js>, value: Value<'js>| -> rquickjs::Result<()> {
            ctx.globals()
                .prop(name, Property::from(value).writable().configurable())
        };
        globals.prop(name, Accessor::new(get, set).configurable())?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Load {
    Pending,
    Done,
    Failed,
}

fn load(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    // SAFETY: bytecode written by the build script for this QuickJS build.
    let module = unsafe { Module::load(ctx.clone(), BYTECODE) }?;
    let (_, promise) = module.eval()?;
    promise.finish::<()>()
}

/// The globals `web.js` defines.
const GLOBALS: [&str; 12] = [
    "TextEncoder",
    "TextDecoder",
    "URL",
    "URLSearchParams",
    "Event",
    "EventTarget",
    "AbortSignal",
    "AbortController",
    "Headers",
    "Request",
    "Response",
    "fetch",
];

/// Settles a `fetch` promise.
pub(crate) fn finish_fetch<'js>(
    ctx: &Ctx<'js>,
    id: RequestId,
    result: Result<HttpResponse, String>,
) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    match result {
        Ok(r) => {
            let flat: Vec<String> = r.headers.into_iter().flat_map(|(k, v)| [k, v]).collect();
            let body = ArrayBuffer::new(ctx.clone(), r.body)?;
            globals
                .get::<_, Function>("__yf_fetch_done")?
                .call::<_, ()>((id, r.status, r.status_text, r.url, r.redirected, flat, body))
        }
        Err(message) => globals
            .get::<_, Function>("__yf_fetch_fail")?
            .call::<_, ()>((id, message)),
    }
}

fn utf8_encode<'js>(ctx: Ctx<'js>, s: String) -> rquickjs::Result<ArrayBuffer<'js>> {
    ArrayBuffer::new(ctx, s.into_bytes())
}

fn utf8_decode<'js>(
    ctx: Ctx<'js>,
    buffer: ArrayBuffer<'js>,
    offset: usize,
    len: usize,
    fatal: bool,
    ignore_bom: bool,
) -> rquickjs::Result<String> {
    // SAFETY: the bytes are read before any JS runs.
    let bytes = unsafe { buffer.as_bytes() }.unwrap_or_default();
    let mut bytes = bytes
        .get(offset..offset.saturating_add(len))
        .unwrap_or_default();
    if !ignore_bom {
        bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    }
    if fatal {
        match std::str::from_utf8(bytes) {
            Ok(s) => Ok(s.to_owned()),
            Err(_) => Err(Exception::throw_type(
                &ctx,
                "The encoded data was not valid utf-8",
            )),
        }
    } else {
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Parses `input` (against `base`), optionally sets one component, and
/// returns the WHATWG URL components, or `null` if parsing or the setter
/// fails.
fn url(
    input: String,
    base: Opt<Value<'_>>,
    field: Opt<String>,
    value: Opt<String>,
) -> Option<Vec<String>> {
    use url::{Url, quirks};
    let base = base
        .0
        .and_then(|b| b.as_string().and_then(|s| s.to_string().ok()));
    let mut url = match base {
        Some(base) => Url::parse(&base).ok()?.join(&input).ok()?,
        None => Url::parse(&input).ok()?,
    };
    if let (Some(field), Some(value)) = (field.0, value.0) {
        let ok = match field.as_str() {
            "href" => quirks::set_href(&mut url, &value).is_ok(),
            "protocol" => quirks::set_protocol(&mut url, &value).is_ok(),
            "username" => quirks::set_username(&mut url, &value).is_ok(),
            "password" => quirks::set_password(&mut url, &value).is_ok(),
            "host" => quirks::set_host(&mut url, &value).is_ok(),
            "hostname" => quirks::set_hostname(&mut url, &value).is_ok(),
            "port" => quirks::set_port(&mut url, &value).is_ok(),
            "pathname" => {
                quirks::set_pathname(&mut url, &value);
                true
            }
            "search" => {
                quirks::set_search(&mut url, &value);
                true
            }
            "hash" => {
                quirks::set_hash(&mut url, &value);
                true
            }
            _ => false,
        };
        if !ok {
            return None;
        }
    }
    Some(vec![
        quirks::href(&url).to_owned(),
        quirks::origin(&url),
        quirks::protocol(&url).to_owned(),
        quirks::username(&url).to_owned(),
        quirks::password(&url).to_owned(),
        quirks::host(&url).to_owned(),
        quirks::hostname(&url).to_owned(),
        quirks::port(&url).to_owned(),
        quirks::pathname(&url).to_owned(),
        quirks::search(&url).to_owned(),
        quirks::hash(&url).to_owned(),
    ])
}
