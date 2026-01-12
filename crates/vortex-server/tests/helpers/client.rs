//! Test client helpers.

use axum::{
    Router,
    body::Body,
    http::{Request, Response, StatusCode, header},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Helper para tests de integracion HTTP.
pub struct TestClient {
    app: Router,
}

impl TestClient {
    /// Crea un nuevo test client con el router proporcionado.
    pub fn new(app: Router) -> Self {
        Self { app }
    }

    /// Hace un GET request.
    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(
            Request::builder()
                .uri(uri)
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Hace un GET request con header Accept personalizado.
    pub async fn get_with_accept(&self, uri: &str, accept: &str) -> TestResponse {
        self.request(
            Request::builder()
                .uri(uri)
                .method("GET")
                .header(header::ACCEPT, accept)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Hace un GET request con headers personalizados.
    pub async fn get_with_headers(&self, uri: &str, headers: Vec<(&str, &str)>) -> TestResponse {
        let mut builder = Request::builder().uri(uri).method("GET");

        for (name, value) in headers {
            builder = builder.header(name, value);
        }

        self.request(builder.body(Body::empty()).unwrap()).await
    }

    /// Ejecuta un request arbitrario.
    async fn request(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("Request failed");

        TestResponse::from_response(response).await
    }
}

/// Wrapper sobre Response con helpers para assertions.
#[derive(Debug)]
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl TestResponse {
    async fn from_response(response: Response<Body>) -> Self {
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to read body")
            .to_bytes()
            .to_vec();

        Self {
            status,
            headers,
            body,
        }
    }

    /// Retorna el body como string.
    pub fn text(&self) -> String {
        String::from_utf8(self.body.clone()).expect("Body is not valid UTF-8")
    }

    /// Parsea el body como JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).expect("Failed to parse JSON")
    }

    /// Retorna un header especifico.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// Verifica que el status sea el esperado.
    pub fn assert_status(&self, expected: StatusCode) -> &Self {
        assert_eq!(
            self.status,
            expected,
            "Expected status {} but got {}. Body: {}",
            expected,
            self.status,
            self.text()
        );
        self
    }

    /// Verifica que el Content-Type contenga el valor esperado.
    pub fn assert_content_type_contains(&self, expected: &str) -> &Self {
        let content_type = self
            .header("content-type")
            .expect("Response missing Content-Type header");

        assert!(
            content_type.contains(expected),
            "Expected Content-Type to contain '{}' but got '{}'",
            expected,
            content_type
        );
        self
    }

    /// Verifica que un header exista.
    pub fn assert_header_exists(&self, name: &str) -> &Self {
        assert!(
            self.headers.contains_key(name),
            "Expected header '{}' to exist",
            name
        );
        self
    }

    /// Verifica que un header tenga un valor especifico.
    pub fn assert_header(&self, name: &str, expected: &str) -> &Self {
        let value = self
            .header(name)
            .unwrap_or_else(|| panic!("Header '{}' not found", name));

        assert_eq!(
            value, expected,
            "Expected header '{}' to be '{}' but got '{}'",
            name, expected, value
        );
        self
    }
}

/// Crea un TestClient con el router por defecto.
pub fn client() -> TestClient {
    TestClient::new(vortex_server::create_router())
}
