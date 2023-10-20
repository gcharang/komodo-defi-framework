use common::{APPLICATION_GRPC_WEB_PROTO, X_GRPC_WEB};
use futures_util::Future;
use http::header::{ACCEPT, CONTENT_TYPE};
use http::{Request, Response};
use mm2_err_handle::prelude::*;
use mm2_net::grpc_web::PostGrpcWebErr;
use mm2_net::wasm::body_stream::ResponseBody;
use mm2_net::wasm::wasm_http::FetchRequest;
use std::{pin::Pin,
          task::{Context, Poll}};
use tonic::body::BoxBody;
use tonic::codegen::Body;
use tower_service::Service;

#[derive(Clone)]
pub struct Client(String);

impl Client {
    pub fn new(url: String) -> Self { Self(url) }
}

impl Service<Request<BoxBody>> for Client {
    type Response = Response<ResponseBody>;

    type Error = MmError<PostGrpcWebErr>;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> { Poll::Ready(Ok(())) }

    fn call(&mut self, request: Request<BoxBody>) -> Self::Future { Box::pin(call(self.0.clone(), request)) }
}

async fn call(mut base_url: String, request: Request<BoxBody>) -> MmResult<Response<ResponseBody>, PostGrpcWebErr> {
    base_url.push_str(&request.uri().to_string());

    let body = request
        .into_body()
        .data()
        .await
        .transpose()
        .map_err(|err| PostGrpcWebErr::Status(err.to_string()))?;
    let body = body.ok_or(MmError::new(PostGrpcWebErr::InvalidRequest(
        "Invalid request body".to_string(),
    )))?;
    Ok(FetchRequest::post(&base_url)
        .body_bytes(body.to_vec())
        .header(CONTENT_TYPE.as_str(), APPLICATION_GRPC_WEB_PROTO)
        .header(ACCEPT.as_str(), APPLICATION_GRPC_WEB_PROTO)
        // https://github.com/grpc/grpc-web/issues/85#issue-217223001
        .header(X_GRPC_WEB, "1")
        .request_stream()
        .await?
        .1)
}
