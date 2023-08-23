// TODO: handle this module inside the `mm2_event_stream` crate.

use hyper::{body::Bytes, Body, Request, Response};
use mm2_core::mm_ctx::MmArc;
use std::convert::Infallible;

pub(crate) const SSE_ENDPOINT: &str = "/event-stream";

/// Handles broadcasted messages from `mm2_event_stream` continuously.
pub async fn handle_sse_events(request: Request<Body>, ctx_h: u32) -> Result<Response<Body>, Infallible> {
    fn get_filtered_events(request: Request<Body>) -> Vec<String> {
        let query = request.uri().query().unwrap_or("");
        let events_param = query
            .split('&')
            .find(|param| param.starts_with("filter="))
            .map(|param| param.trim_start_matches("filter="))
            .unwrap_or("");

        events_param.split(',').map(|event| event.to_string()).collect()
    }

    let ctx = match MmArc::from_ffi_handle(ctx_h) {
        Ok(ctx) => ctx,
        Err(err) => return handle_internal_error(err).await,
    };

    let filtered_events = get_filtered_events(request);

    let mut channel_controller = ctx.stream_channel_controller.clone();
    let mut rx = channel_controller.create_channel(1); // TODO: read this from configuration
    let body = Body::wrap_stream(async_stream::stream! {
        while let Some(msg) = rx.recv().await {
            // If there are no filtered events, that means we want to
            // stream out all the events.
            if filtered_events.is_empty() || filtered_events.contains(&msg.event_type().to_owned()) {
                let Ok(json) = serde_json::to_string(&msg) else { continue }; // TODO: This is not a good idea. Refactor the event type.
                yield Ok::<_, hyper::Error>(Bytes::from(format!("data: {json} \n\n")));
            }
        }
    });

    let response = Response::builder()
        .status(200)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Access-Control-Allow-Origin", "*") // TODO: read this from configuration
        .body(body);

    match response {
        Ok(res) => Ok(res),
        Err(err) => return handle_internal_error(err.to_string()).await,
    }
}

async fn handle_internal_error(message: String) -> Result<Response<Body>, Infallible> {
    let response = Response::builder()
        .status(500)
        .body(Body::from(message))
        .expect("Returning 500 should never fail.");

    Ok(response)
}
