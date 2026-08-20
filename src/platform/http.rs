//! HTTP (MAPPS-504).
//!
//! The surface is the subset of `gloo-net`'s builder that
//! `crate::hooks::fetch::api` already speaks - `Request::get(url)`,
//! `.header()`, `.json()`, `.send()`, then `status()` / `ok()` /
//! `text()` / `json()` / `binary()` on the response - so the browser
//! build keeps using `gloo-net` verbatim and the call sites only change
//! which module they import it from.
//!
//! Desktop maps the same surface onto `reqwest`. `multipart_file` is the
//! one method `gloo-net` does not have: the tenant-logo `PUT`
//! (MAPPS-429) needs a `multipart/form-data` body whose boundary the
//! client picks, which is `FormData` in a browser and
//! `reqwest::multipart` on the desktop.

#[cfg(target_arch = "wasm32")]
mod imp {
    pub use gloo_net::http::{Request, RequestBuilder, Response};
    pub type Error = gloo_net::Error;

    use wasm_bindgen::JsCast;

    /// Attach a single file as a `multipart/form-data` body.
    ///
    /// The `Content-Type` header is deliberately NOT set: the browser
    /// derives it from the `FormData` and appends the boundary. Setting
    /// it by hand omits the boundary and the server cannot split the
    /// body (the MAPPS-429 note in `hooks::fetch`).
    pub trait MultipartExt {
        fn multipart_file(
            self,
            file_name: &str,
            mime: &str,
            bytes: &[u8],
        ) -> Result<Request, Error>;
    }

    impl MultipartExt for RequestBuilder {
        fn multipart_file(
            self,
            file_name: &str,
            mime: &str,
            bytes: &[u8],
        ) -> Result<Request, Error> {
            let array = js_sys::Uint8Array::from(bytes);
            let parts = js_sys::Array::new();
            parts.push(&array.buffer());
            let opts = web_sys::BlobPropertyBag::new();
            opts.set_type(mime);
            let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
                .map_err(|_| js_error("could not prepare the upload (blob)"))?;
            let form = web_sys::FormData::new()
                .map_err(|_| js_error("could not prepare the upload (form)"))?;
            form.append_with_blob_and_filename("file", &blob, file_name)
                .map_err(|_| js_error("could not prepare the upload (part)"))?;
            self.body(form.unchecked_into::<wasm_bindgen::JsValue>())
        }
    }

    /// `gloo_net::Error` has no "something in the DOM said no" variant,
    /// and the `JsValue` a failed `web_sys` call rejects with carries no
    /// message worth forwarding. Carry our own text instead of dropping
    /// the failure into a generic one.
    fn js_error(what: &str) -> Error {
        gloo_net::Error::GlooError(what.to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use serde::Serialize;
    use std::sync::OnceLock;

    /// Transport failure. Display is what reaches the toast, so it says
    /// what failed rather than naming a crate.
    #[derive(Debug, Clone)]
    pub struct Error(String);

    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }

    impl std::error::Error for Error {}

    impl Error {
        fn new(msg: impl std::fmt::Display) -> Self {
            Self(msg.to_string())
        }
    }

    /// One pooled client for the process. Built once; a build failure
    /// (no usable TLS backend, for instance) is kept and returned from
    /// every `send()` rather than panicking the UI thread or being
    /// retried per request.
    fn client() -> Result<&'static reqwest::Client, Error> {
        static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
        CLIENT
            .get_or_init(|| {
                reqwest::Client::builder()
                    .build()
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(|e| Error(format!("could not start the HTTP client: {e}")))
    }

    enum Body {
        Empty,
        Raw(Vec<u8>),
        Json(Vec<u8>),
        Multipart {
            file_name: String,
            mime: String,
            bytes: Vec<u8>,
        },
    }

    pub struct RequestBuilder {
        method: reqwest::Method,
        url: String,
        headers: Vec<(String, String)>,
    }

    pub struct Request {
        builder: RequestBuilder,
        body: Body,
    }

    impl Request {
        pub fn get(url: &str) -> RequestBuilder {
            RequestBuilder::new(reqwest::Method::GET, url)
        }
        pub fn post(url: &str) -> RequestBuilder {
            RequestBuilder::new(reqwest::Method::POST, url)
        }
        pub fn put(url: &str) -> RequestBuilder {
            RequestBuilder::new(reqwest::Method::PUT, url)
        }
        pub fn patch(url: &str) -> RequestBuilder {
            RequestBuilder::new(reqwest::Method::PATCH, url)
        }
        pub fn delete(url: &str) -> RequestBuilder {
            RequestBuilder::new(reqwest::Method::DELETE, url)
        }

        pub async fn send(self) -> Result<Response, Error> {
            let Self { builder, body } = self;
            let mut req = client()?.request(builder.method, &builder.url);
            for (k, v) in &builder.headers {
                req = req.header(k.as_str(), v.as_str());
            }
            req = match body {
                Body::Empty => req,
                Body::Raw(bytes) | Body::Json(bytes) => req.body(bytes),
                Body::Multipart {
                    file_name,
                    mime,
                    bytes,
                } => {
                    let part = reqwest::multipart::Part::bytes(bytes)
                        .file_name(file_name)
                        .mime_str(&mime)
                        .map_err(|e| Error::new(format!("could not prepare the upload: {e}")))?;
                    // reqwest writes `Content-Type` with the boundary it
                    // generated; anything we set by hand is dropped here
                    // for the same reason the browser path leaves it alone.
                    req.multipart(reqwest::multipart::Form::new().part("file", part))
                }
            };
            let resp = req.send().await.map_err(Error::new)?;
            let status = resp.status().as_u16();
            // Header names are lowercased so `get` is case-insensitive
            // the way `gloo_net`'s is; a value that is not valid UTF-8 is
            // dropped rather than lossily converted, since every header
            // read here feeds a filename or a content type.
            let headers = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| {
                    v.to_str()
                        .ok()
                        .map(|v| (k.as_str().to_ascii_lowercase(), v.to_string()))
                })
                .collect();
            let body = resp.bytes().await.map_err(Error::new)?.to_vec();
            Ok(Response {
                status,
                headers,
                body,
            })
        }
    }

    impl RequestBuilder {
        fn new(method: reqwest::Method, url: &str) -> Self {
            Self {
                method,
                url: url.to_string(),
                headers: Vec::new(),
            }
        }

        #[must_use]
        pub fn header(mut self, key: &str, value: &str) -> Self {
            self.headers.push((key.to_string(), value.to_string()));
            self
        }

        /// Raw body, for the one caller that sends
        /// `application/x-www-form-urlencoded` (the OIDC token endpoint).
        /// Fallible to match `gloo_net`'s signature, which can fail
        /// converting the value into a JS body.
        pub fn body(self, body: impl Into<Vec<u8>>) -> Result<Request, Error> {
            Ok(Request {
                builder: self,
                body: Body::Raw(body.into()),
            })
        }

        pub fn json<T: Serialize + ?Sized>(self, value: &T) -> Result<Request, Error> {
            let bytes = serde_json::to_vec(value)
                .map_err(|e| Error::new(format!("could not encode the request body: {e}")))?;
            Ok(Request {
                builder: self,
                body: Body::Json(bytes),
            })
        }

        pub async fn send(self) -> Result<Response, Error> {
            Request {
                builder: self,
                body: Body::Empty,
            }
            .send()
            .await
        }
    }

    pub trait MultipartExt {
        fn multipart_file(
            self,
            file_name: &str,
            mime: &str,
            bytes: &[u8],
        ) -> Result<Request, Error>;
    }

    impl MultipartExt for RequestBuilder {
        fn multipart_file(
            self,
            file_name: &str,
            mime: &str,
            bytes: &[u8],
        ) -> Result<Request, Error> {
            Ok(Request {
                builder: self,
                body: Body::Multipart {
                    file_name: file_name.to_string(),
                    mime: mime.to_string(),
                    bytes: bytes.to_vec(),
                },
            })
        }
    }

    /// The body is buffered at `send()` so the readers below take
    /// `&self`, matching `gloo_net::http::Response`. `hooks::fetch`
    /// relies on that: `handle_response` calls `json()` on the success
    /// branch and `text()` on the error branch of the same response.
    pub struct Response {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    }

    /// The response headers, mirroring `gloo_net::http::Headers`'s
    /// `get`.
    pub struct Headers<'a> {
        pairs: &'a [(String, String)],
    }

    impl Headers<'_> {
        pub fn get(&self, name: &str) -> Option<String> {
            let name = name.to_ascii_lowercase();
            self.pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.clone())
        }
    }

    impl Response {
        pub fn status(&self) -> u16 {
            self.status
        }

        pub fn ok(&self) -> bool {
            (200..300).contains(&self.status)
        }

        pub fn headers(&self) -> Headers<'_> {
            Headers {
                pairs: &self.headers,
            }
        }

        pub async fn text(&self) -> Result<String, Error> {
            Ok(String::from_utf8_lossy(&self.body).into_owned())
        }

        pub async fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, Error> {
            serde_json::from_slice(&self.body).map_err(Error::new)
        }

        pub async fn binary(&self) -> Result<Vec<u8>, Error> {
            Ok(self.body.clone())
        }
    }
}

pub use imp::{Error, MultipartExt, Request, RequestBuilder, Response};
