//! What Android's server service calls (`server/NativeServer.java`): the
//! dedicated server of `dedicated`, in the service's own process. Nothing of
//! Java is kept across calls; text goes both ways, JSON where it is more.

use crate::dedicated;
use crate::server::ServerOptions;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;

fn text(env: &mut JNIEnv, value: &str) -> jstring {
    match env.new_string(value) {
        Ok(s) => s.into_raw(),
        Err(e) => {
            let _ = env.throw_new("java/lang/IllegalStateException", e.to_string());
            std::ptr::null_mut()
        }
    }
}

/// `options` as JSON (`ServerOptions`); the answer is empty, or why not.
#[no_mangle]
pub extern "system" fn Java_com_dicefoxcreations_anotherworldbuilder_server_NativeServer_start(mut env: JNIEnv, _: JClass, options: JString) -> jstring {
    let started = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let options: String = env.get_string(&options).map_err(|e| e.to_string())?.into();
        dedicated::start(serde_json::from_str::<ServerOptions>(&options).map_err(|e| e.to_string())?)
    }))
    .unwrap_or_else(|_| Err("The server could not start.".into()));
    text(&mut env, &started.err().unwrap_or_default())
}

#[no_mangle]
pub extern "system" fn Java_com_dicefoxcreations_anotherworldbuilder_server_NativeServer_stop(_: JNIEnv, _: JClass) {
    let _ = std::panic::catch_unwind(dedicated::stop);
}

/// `dedicated::Status` as JSON.
#[no_mangle]
pub extern "system" fn Java_com_dicefoxcreations_anotherworldbuilder_server_NativeServer_status(mut env: JNIEnv, _: JClass) -> jstring {
    text(&mut env, &serde_json::to_string(&dedicated::status()).unwrap_or_default())
}

#[no_mangle]
pub extern "system" fn Java_com_dicefoxcreations_anotherworldbuilder_server_NativeServer_version(mut env: JNIEnv, _: JClass) -> jstring {
    text(&mut env, env!("CARGO_PKG_VERSION"))
}

/// `{ worlds, error }` of the server folder `data`.
#[no_mangle]
pub extern "system" fn Java_com_dicefoxcreations_anotherworldbuilder_server_NativeServer_worlds(mut env: JNIEnv, _: JClass, data: JString) -> jstring {
    let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let data: String = env.get_string(&data).map_err(|e| e.to_string())?.into();
        dedicated::worlds(std::path::Path::new(&data))
    }))
    .unwrap_or_else(|_| Err("The worlds could not be read.".into()));
    let reply = match read {
        Ok(worlds) => serde_json::json!({ "worlds": worlds, "error": "" }),
        Err(error) => serde_json::json!({ "worlds": [], "error": error }),
    };
    text(&mut env, &reply.to_string())
}
