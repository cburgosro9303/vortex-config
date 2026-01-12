//! Middleware stack para el servidor HTTP.
//!
//! Este modulo contiene los middleware de Tower que se aplican a todas las requests:
//! - `RequestIdLayer`: Genera/propaga X-Request-Id
//! - `LoggingLayer`: Logging estructurado de requests

mod logging;
mod request_id;

pub use logging::{LoggingLayer, LoggingMiddleware};
pub use request_id::{REQUEST_ID_HEADER, RequestIdLayer, RequestIdMiddleware};
