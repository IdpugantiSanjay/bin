#![deny(clippy::pedantic)]
#![allow(clippy::unused_async)]

mod errors;
mod highlight;
mod io;
mod params;

use crate::{
    errors::{InternalServerError, NotFound},
    highlight::highlight,
    params::{HostHeader, IsPlaintextRequest},
};

use actix_web::{
    http::header,
    web::{self, Bytes, Data, FormConfig, PayloadConfig},
    App, Error, HttpRequest, HttpResponse, HttpServer, Responder,
};
use askama::{Html as AskamaHtml, MarkupDisplay, Template};
use io::{generate_id, Store};
use log::{error, info};
use once_cell::sync::Lazy;
use sqlx::SqlitePool;
use core::str;
use std::{
    borrow::Cow, net::{IpAddr, Ipv4Addr, SocketAddr}, path::Path,
};
use actix_web::web::{redirect, Redirect};
use argh::FlagInfoKind::Option;
use syntect::html::{css_for_theme_with_class_style, ClassStyle};
// use crate::io::{delete_paste, update_paste, SerializableStore};

#[derive(argh::FromArgs, Clone)]
/// a pastebin.
pub struct BinArgs {
    /// socket address to bind to (default: 127.0.0.1:8820)
    #[argh(
        positional,
        default = "SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8820)"
    )]
    bind_addr: SocketAddr,
    /// maximum amount of pastes to store before rotating (default: 1000)
    /// maximum paste size in bytes (default. 32kB)
    #[argh(option, default = "32 * 1024")]
    max_paste_size: usize,
    /// file path to store pastes (default. store.json)
    #[argh(option, default = "\"./store.db\".to_string()")]
    store_path: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "DEBUG");
    }
    pretty_env_logger::init();

    let args: BinArgs = argh::from_env();

    let database_url = format!("sqlite://{}", args.store_path);
    let pool = SqlitePool::connect(database_url.as_str())
        .await
        .expect("Failed to create SQLite pool.");

    let store: Store = Store::new(pool);

    let server = HttpServer::new({
        let args = args.clone();

        move || {
            App::new()
                .app_data(Data::new(store.clone()))
                .app_data(PayloadConfig::default().limit(args.max_paste_size))
                .app_data(FormConfig::default().limit(args.max_paste_size))
                .wrap(actix_web::middleware::Compress::default())
                .route("/", web::get().to(index))
                .route("/", web::post().to(submit))
                .route("/", web::put().to(submit_raw))
                .route("/", web::head().to(HttpResponse::MethodNotAllowed))
                .route("/highlight.css", web::get().to(highlight_css))
                .route("/pastes", web::get().to(show_pastes))
                .route("/{paste}", web::get().to(show_paste))
                .route("/remove_paste/{paste}", web::post().to(remove_paste))
                .route("/edit/{paste}", web::get().to(edit_paste))
                .route("/edit/{paste}", web::post().to(update_paste_content))
                .route("/{paste}", web::head().to(HttpResponse::MethodNotAllowed))
                .default_service(web::to(|req: HttpRequest| async move {
                    error!("Couldn't find resource {}", req.uri());
                    HttpResponse::from_error(NotFound)
                }))
        }
    });

    info!("Listening on http://{}", args.bind_addr);

    server.bind(args.bind_addr)?.run().await
}

#[derive(Template)]
#[template(path = "index.html")]
struct Index;

async fn index(req: HttpRequest) -> Result<HttpResponse, Error> {
    render_template(&req, &Index)
}

#[derive(serde::Deserialize)]
struct IndexForm {
    val: Bytes,
}

async fn submit(input: web::Form<IndexForm>, store: Data<Store>) -> impl Responder {
    let id = generate_id();
    let uri = format!("/{id}");

    let result = store.insert(&id, str::from_utf8(&input.into_inner().val).unwrap()).await;
    if result.is_err() {
        return HttpResponse::InternalServerError().into();
    }
    
    HttpResponse::Found()
        .append_header((header::LOCATION, uri))
        .finish()
}

async fn submit_raw(
    data: Bytes,
    host: HostHeader,
    store: Data<Store>,
) -> Result<String, Error> {
    let id = generate_id();
    let uri = if let Some(Ok(host)) = host.0.as_ref().map(|v| std::str::from_utf8(v.as_bytes())) {
        format!("https://{host}/{id}\n")
    } else {
        format!("/{id}\n")
    };

    let result = store.insert(&id, str::from_utf8(&data).unwrap()).await;

    match result {
        Ok(_) => Ok(uri),
        Err(err) => Err(actix_web::error::ErrorInternalServerError(err)),
    }
}

#[derive(Template)]
#[template(path = "paste.html")]
struct ShowPaste<'a> {
    content: MarkupDisplay<AskamaHtml, Cow<'a, String>>,
    paste: String
}


#[derive(Template)]
#[template(path = "pastes.html")]
struct ShowPastes<'a> {
    content: MarkupDisplay<AskamaHtml, Cow<'a, String>>,
}

#[derive(Template)]
#[template(path = "edit.html")]
struct EditPaste<'a> {
    paste: &'a str,
    content: MarkupDisplay<AskamaHtml, Cow<'a, String>>,
}

async fn remove_paste(req: HttpRequest,
                      key: actix_web::web::Path<String>, store: Data<Store>) -> Result<HttpResponse, Error> {
    let mut splitter = key.splitn(2, '.');
    let key = splitter.next().unwrap();
    let _ = splitter.next();

    let result = store.delete_paste_by_title(&key).await;

    match result {
        Ok(_) => {
            return Ok(HttpResponse::Found()
                .append_header(("LOCATION", "/pastes"))
                .finish());
        }
        Err(_) => {
            return Ok(HttpResponse::NotFound().into());
        }
    }
}

async fn show_paste(
    req: HttpRequest,
    key: actix_web::web::Path<String>,
    plaintext: IsPlaintextRequest,
    store: Data<Store>,
) -> Result<HttpResponse, Error> {
    let mut splitter = key.splitn(2, '.');
    let key = splitter.next().unwrap();
    let ext = splitter.next();

    info!("Fetching paste with id {}", key);
    let entry = store.get_paste_by_title(&key).await;

    let response = match entry {
        Ok(entry) => {
            if *plaintext {
                Ok(HttpResponse::Ok()
                    .content_type("text/plain; charset=utf-8")
                    .body(entry.content))
            } else {
                let code_highlighted = match ext {
                    Some(extension) => match highlight(&entry.content, extension) {
                        Some(html) => html,
                        None => return Err(NotFound.into()),
                    },
                    None => htmlescape::encode_minimal(&entry.content),
                };
        
                // Add <code> tags to enable line numbering with CSS
                let html = format!(
                    "<code>{}</code>",
                    code_highlighted.replace('\n', "</code><code>")
                );
        
                let content = MarkupDisplay::new_safe(Cow::Borrowed(&html), AskamaHtml);
        
                render_template(&req, &ShowPaste { content, paste: key.to_string() })
            }
        },
        Err(_) => return Ok(HttpResponse::InternalServerError().into()),
    };
    response
}

async fn edit_paste(req: HttpRequest, key: actix_web::web::Path<String>, store: Data<Store>) -> Result<HttpResponse, Error> {
    let mut splitter = key.splitn(2, '.');
    let paste_id = splitter.next().unwrap();

    let paste = store.get_paste_by_title(&key).await;

    let response = match paste {
        Ok(paste) => {
            let content = MarkupDisplay::new_safe(Cow::Borrowed(&paste.content), AskamaHtml);
            render_template(&req, &EditPaste { paste: paste_id, content })
        },
        Err(_) => return Ok(HttpResponse::NotFound().into()),
    };
    response
}

async fn update_paste_content(req: HttpRequest, key: actix_web::web::Path<String>, input: web::Form<IndexForm>, store: Data<Store>) -> impl Responder {
    let mut splitter = key.splitn(2, '.');
    let paste = splitter.next().unwrap();
    let uri = format!("/{paste}");


    let result = store.update_paste_content(&paste, str::from_utf8(&input.into_inner().val).unwrap()).await;

    match result {
        Ok(_) => {
            return Ok::<HttpResponse, Error>(HttpResponse::Found()
                .append_header(("LOCATION", uri))
                .finish());
        }
        Err(_) => {
            return Ok(HttpResponse::NotFound().into());
        }
    }
}

async fn show_pastes(req: HttpRequest, store: Data<Store>) -> Result<HttpResponse, Error> {
    let result = store.get_all_pastes().await;
    if result.is_err() {
        return Ok(HttpResponse::InternalServerError().into());
    }

    let links: Vec<String> = result.unwrap().into_iter().map(|p| format!(
        "<li><form action=\"/remove_paste/{}\" method=\"POST\"><a href=\"/{}.md\">{}</a><button type=\"submit\" title=\"Delete {}\">&#x274C;</button><button type=\"button\" title=\"Edit {}\"><a href=\"/edit/{}\">✏️</a></button></form></li>", 
        p.title, p.title, p.title, p.title, p.title, p.title)).collect();

    let html = format!(
        "<ul>{}</ul>",
        links.join("\n")
    );
    let content = MarkupDisplay::new_safe(Cow::Borrowed(&html), AskamaHtml);
    render_template(&req, &ShowPastes { content })
}

async fn highlight_css() -> HttpResponse {
    static CSS: Lazy<Bytes> = Lazy::new(|| {
        highlight::BAT_ASSETS.with(|s| {
            Bytes::from(
                css_for_theme_with_class_style(s.get_theme("OneHalfDark"), ClassStyle::Spaced)
                    .unwrap(),
            )
        })
    });

    HttpResponse::Ok()
        .content_type("text/css")
        .body(CSS.clone())
}

fn render_template<T: Template>(req: &HttpRequest, template: &T) -> Result<HttpResponse, Error> {
    match template.render() {
        Ok(html) => Ok(HttpResponse::Ok().content_type("text/html").body(html)),
        Err(e) => {
            error!("Error while rendering template for {}: {e}", req.uri());
            Err(InternalServerError(Box::new(e)).into())
        }
    }
}
