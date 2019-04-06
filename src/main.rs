use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::process;
use std::str;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hyper::{Body, Request, Response};
use hyper::rt::Future;
use hyper::service::service_fn_ok;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};

extern crate hyper;
extern crate notify;
extern crate open;
extern crate pulldown_cmark;
extern crate sha2;
fn main() { let args: Vec<String> = env::args().collect(); if args.len() < 2 {
        println!("Must supply file name");
        process::exit(2);
    }
    let md_filename = &args[1];

    let (notify_tx, notify_rx): (Sender<String>, Receiver<String>) = channel();
    let (render_tx, render_rx): (
        Sender<Result<Rendered, String>>,
        Receiver<Result<Rendered, String>>,
    ) = channel();

    let filewatcher = FileWatcher::new(md_filename.to_string(), notify_tx);
    let _filewatcher_handle = thread::spawn(move || {
        filewatcher.watch().expect("Failed to create file watcher");
    });

    let renderer = Renderer::new(notify_rx, render_tx);
    let _renderer_handle = thread::spawn(move || {
        renderer.render_on_recv();
    });

    let server = Server::new(md_filename.to_string());
    server.serve(render_rx).expect("Server failed!");
}

struct FileWatcher {
    filepath: String,
    tx: Sender<String>,
}

impl FileWatcher {
    fn new(filepath: String, tx: Sender<String>) -> FileWatcher {
        FileWatcher { tx, filepath }
    }

    fn watch(&self) -> Result<(), String> {
        let (notify_tx, notify_rx) = channel();
        let mut watcher: RecommendedWatcher =
            Watcher::new(notify_tx, Duration::from_secs(2)).map_err(|err| err.to_string())?;
        watcher
            .watch(&self.filepath, RecursiveMode::Recursive)
            .map_err(|err| err.to_string())?;
        loop {
            match notify_rx.recv() {
                Ok(_event) => self.tx.send(self.filepath.clone()).expect("couldn't write to watcher channel"),
                Err(e) => println!("Error: {:?}", e),
            };
        }
    }
}

struct Renderer {
    rx: Receiver<String>,
    tx: Sender<Result<Rendered, String>>,
}

impl Renderer {
    fn new(rx: Receiver<String>, tx: Sender<Result<Rendered, String>>) -> Renderer {
        Renderer { rx: rx, tx: tx }
    }

    fn render_on_recv(&self) {
        loop {
            let res = match self.rx.recv() {
                Ok(filepath) => render(filepath),
                Err(e) => Err(format!("Error reading from channel: {:?}", e)),
            };
            self.tx.send(res).expect("couldn't write to render channel");
        }
    }
}

struct Rendered {
    path: String,
    contents: String,
    hash: String,
}

fn render(filepath: String) -> Result<Rendered, String> {
    let path = filepath.clone();
    let mut f = File::open(filepath).map_err(|err| err.to_string())?;
    let mut file_contents = String::new();
    f.read_to_string(&mut file_contents)
        .map_err(|err| err.to_string())?;
    let parser = pulldown_cmark::Parser::new(file_contents.as_str());
    let mut contents = String::new();
    pulldown_cmark::html::push_html(&mut contents, parser);
    let hash = generate_hash(contents.clone())?;
    Ok(Rendered {
        path: path,
        contents: contents,
        hash: hash,
    })
}

fn generate_hash(content: String) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.input(content);
    let res = hasher.result();
    str::from_utf8(&res[..])
        .map(|s| s.to_string())
        .map_err(|err| err.to_string())
}

struct Server {
    rendered: Arc<Mutex<Result<Rendered, String>>>,
}

impl Server {
    fn new(filepath: String) -> Server {
        Server {
            rendered: Arc::new(Mutex::new(render(filepath))),
        }
    }

    fn serve(&self, rx: Receiver<Result<Rendered, String>>) -> Result<(), String> {
        let rendered = Arc::clone(&self.rendered);
        let _ = thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(s) => {
                        let mut rendered = rendered.lock().unwrap();
                        *rendered = s;
                    },
                    Err(e) => println!("Error receiving from channel: {:?}", e),
                }
            }
        });

        // actually serve the file
        let addr = ([127, 0, 0, 1], 3000).into();
        let handler = self.content_handler();
        let new_svc = || {
            service_fn_ok()
        };

        let srv = hyper::Server::bind(&addr)
            .serve(new_svc)
            .map_err(|e| eprintln!("server error: {}", e));

        hyper::rt::run(srv);

        Ok(())
    }

    fn content_handler(&self) -> impl FnMut(Request<Body>) -> Response<Body> {
        let rendered = Arc::clone(&self.rendered);
        |_req: Request<Body>| {
            
            let rendered = rendered.clone().lock().unwrap();

            let resp_body = match *rendered {
                Ok(r) => r.contents,
                Err(e) => e,
            };
            Response::new(Body::from(resp_body))
        }
    }

    fn hash_handler(&self, _req: Request<Body>) -> Response<Body> {
        let rendered = self.rendered.lock().unwrap();

        let resp_body = match *rendered {
            Ok(r) => r.hash,
            Err(e) => e,
        };       

        Response::new(Body::from(resp_body))
    }
}

fn md_html_wrapper(content: String) -> String {
    format!(
        r##"<!doctype html>
<html>
    <head>
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <link rel="stylesheet" href="https://sindresorhus.com/github-markdown-css/github-markdown.css">
        <style>
	.markdown-body {{
		box-sizing: border-box;
		min-width: 200px;
		max-width: 980px;
		margin: 0 auto;
		padding: 45px;
	}}

	@media (max-width: 767px) {{
		.markdown-body {{
			padding: 15px;
                }}
        }}
        </style>
    </head>
    <body>
        <div class="markdown-body">
            {}
        </div>
    </body>
</html>
            "##,
        content
    )
}
