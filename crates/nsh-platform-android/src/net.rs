//! `fetch` on Java's `HttpURLConnection` (`Net.java`), which honors the
//! system's proxy and trust store settings.

use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicI64, Ordering},
};

use jni::{
    JNIEnv,
    objects::{JByteArray, JClass, JObject, JObjectArray, JString, JValue},
    signature::{Primitive, ReturnType},
    sys::{jboolean, jint, jlong},
};
use nsh_engine::{CancelFetch, FetchDone, HttpRequest, HttpResponse, Network};
use rustc_hash::FxHashMap;

use crate::java::{java, with_env};

/// Requests in flight, by key.
static PENDING: Mutex<Option<FxHashMap<i64, FetchDone>>> = Mutex::new(None);
static NEXT: AtomicI64 = AtomicI64::new(1);

pub(crate) fn network() -> Arc<dyn Network> {
    static NETWORK: OnceLock<Arc<JavaNetwork>> = OnceLock::new();
    NETWORK.get_or_init(|| Arc::new(JavaNetwork)).clone()
}

struct JavaNetwork;

impl Network for JavaNetwork {
    fn fetch(&self, request: HttpRequest, done: FetchDone) -> CancelFetch {
        let key = NEXT.fetch_add(1, Ordering::Relaxed);
        PENDING
            .lock()
            .expect("pending")
            .get_or_insert_with(FxHashMap::default)
            .insert(key, done);
        with_env(|env| {
            let j = java();
            let method = env.new_string(&request.method)?;
            let url = env.new_string(&request.url)?;
            let headers = env.new_object_array(
                (request.headers.len() * 2) as i32,
                "java/lang/String",
                JObject::null(),
            )?;
            for (i, (k, v)) in request.headers.iter().enumerate() {
                let k = env.new_string(k)?;
                let v = env.new_string(v)?;
                env.set_object_array_element(&headers, (i * 2) as i32, k)?;
                env.set_object_array_element(&headers, (i * 2 + 1) as i32, v)?;
            }
            let body = if request.body.is_empty() {
                JObject::null()
            } else {
                JObject::from(env.byte_array_from_slice(&request.body)?)
            };
            // SAFETY: matches the signature checked in `java::init`.
            unsafe {
                env.call_static_method_unchecked(
                    <&JClass<'_>>::from(j.net.as_obj()),
                    j.net_fetch,
                    ReturnType::Primitive(Primitive::Void),
                    &[
                        JValue::Long(key).as_jni(),
                        JValue::Object(&method).as_jni(),
                        JValue::Object(&url).as_jni(),
                        JValue::Object(&headers).as_jni(),
                        JValue::Object(&body).as_jni(),
                        JValue::Bool(request.follow_redirects.into()).as_jni(),
                    ],
                )
            }
            .map(|_| ())
        });
        Box::new(move || {
            // Dropping the callback: a late completion is ignored.
            take(key);
            with_env(|env| {
                let j = java();
                // SAFETY: matches the signature checked in `java::init`.
                unsafe {
                    env.call_static_method_unchecked(
                        <&JClass<'_>>::from(j.net.as_obj()),
                        j.net_cancel,
                        ReturnType::Primitive(Primitive::Void),
                        &[JValue::Long(key).as_jni()],
                    )
                }
                .map(|_| ())
            });
        })
    }
}

fn take(key: i64) -> Option<FetchDone> {
    PENDING.lock().expect("pending").as_mut()?.remove(&key)
}

/// `Net.nativeDone`: a request finished (`error` non-null on failure).
#[unsafe(no_mangle)]
extern "system" fn Java_dev_nsh_Net_nativeDone(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    key: jlong,
    status: jint,
    url: JString<'_>,
    redirected: jboolean,
    headers: JObjectArray<'_>,
    body: JByteArray<'_>,
    error: JString<'_>,
) {
    let Some(done) = take(key) else { return };
    if !error.is_null() {
        let message = env.get_string(&error).map(String::from).unwrap_or_default();
        return done(Err(message));
    }
    let result = (|| -> jni::errors::Result<HttpResponse> {
        let url = String::from(env.get_string(&url)?);
        let count = env.get_array_length(&headers)?;
        let mut pairs = Vec::with_capacity(count as usize / 2);
        for i in (0..count - 1).step_by(2) {
            let k: JString<'_> = env.get_object_array_element(&headers, i)?.into();
            let v: JString<'_> = env.get_object_array_element(&headers, i + 1)?.into();
            pairs.push((
                String::from(env.get_string(&k)?),
                String::from(env.get_string(&v)?),
            ));
        }
        let body = if body.is_null() {
            Vec::new()
        } else {
            env.convert_byte_array(&body)?
        };
        Ok(HttpResponse {
            status: status as u16,
            status_text: String::new(),
            url,
            redirected: redirected != 0,
            headers: pairs,
            body,
        })
    })();
    done(result.map_err(|e| e.to_string()));
}
