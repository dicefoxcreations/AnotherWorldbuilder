//! The test bench: the same core, only over HTTP instead of Tauri.
//!
//! The interface can be used in a browser — typing, clicking, hovering with
//! the mouse — and behind it runs **the same** Rust code as in the app. A
//! stub with made-up answers would not have shown exactly the bugs that turn
//! up in use: that a half-typed `<` swallows the `</body>`, for example.
//!
//! Deliberately tiny: no web framework, no dependency, no state. Only the
//! three commands the template editor calls on every keystroke.
//!
//! ```text
//! cargo run -p aw-testbench           # listens on 127.0.0.1:5320
//! ```
//!
//! The server listens only on `localhost` and has no access to worlds, keys
//! or files — it knows nothing but the text the browser sends it.

use aw_model::{editor, markdown, template::Template, template_text, template_text::Directory};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

const ADDRESS: &str = "127.0.0.1:5320";

fn main() {
    let listener = TcpListener::bind(ADDRESS).unwrap_or_else(|e| {
        eprintln!("the test bench cannot open {ADDRESS}: {e}");
        std::process::exit(1);
    });
    println!("test bench listening on http://{ADDRESS}");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = serve(s) {
                    eprintln!("request failed: {e}");
                }
            }
            Err(e) => eprintln!("connection failed: {e}"),
        }
    }
}

fn serve(mut stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut line = String::new();
    reader.read_line(&mut line)?;
    let method = line.split_whitespace().next().unwrap_or("").to_string();

    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some(value) = header.to_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }

    // The browser asks for permission before the actual call.
    if method == "OPTIONS" {
        return respond(&mut stream, "200 OK", "");
    }

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    let answer = match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(request) => compute(&request),
        Err(e) => serde_json::json!({ "error": e.to_string() }),
    };
    respond(&mut stream, "200 OK", &answer.to_string())
}

/// The same commands the app offers through Tauri.
fn compute(request: &serde_json::Value) -> serde_json::Value {
    let command = request["cmd"].as_str().unwrap_or_default();
    let args = &request["args"];

    // Which types exist only the page knows — the test bench has no world.
    // It sends the directory along with every call.
    let directory = Directory::new(
        args["directory"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        Some((
                            e["id"].as_str()?.to_string(),
                            e["name"].as_str()?.to_string(),
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    );

    let result = match command {
        "template_read" => {
            let text = args["text"].as_str().unwrap_or_default();
            // What the supertypes and groups bring along may be bound as well.
            let others: Vec<Template> = serde_json::from_value(args["others"].clone()).unwrap_or_default();
            match serde_json::from_value::<Template>(args["base"].clone()) {
                Ok(base) => serde_json::to_value(editor::read(text, base, &directory, &others)),
                Err(e) => return serde_json::json!({ "error": format!("base: {e}") }),
            }
        }
        "template_write" => match serde_json::from_value::<Template>(args["template"].clone()) {
            Ok(v) => Ok(serde_json::Value::String(template_text::to_text(&v, &directory))),
            Err(e) => return serde_json::json!({ "error": format!("template: {e}") }),
        },
        "markdown_read" => {
            serde_json::to_value(markdown::split(args["text"].as_str().unwrap_or_default()))
        }
        // Nothing else exists at the test bench: it knows no worlds and no
        // keys. A `null` is more honest than a made-up answer the interface
        // would cling to.
        _ => Ok(serde_json::Value::Null),
    };

    match result {
        Ok(value) => serde_json::json!({ "value": value }),
        Err(e) => serde_json::json!({ "error": e.to_string() }),
    }
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Headers: content-type\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\n\
         Connection: close\r\n\r\n{body}",
        body.as_bytes().len()
    )?;
    stream.flush()
}
