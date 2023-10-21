use crate::grpc_web::PostGrpcWebErr;

use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use byteorder::{BigEndian, ByteOrder};
use bytes::{BufMut, Bytes, BytesMut};
use common::{APPLICATION_GRPC_WEB, APPLICATION_GRPC_WEB_PROTO, APPLICATION_GRPC_WEB_TEXT,
             APPLICATION_GRPC_WEB_TEXT_PROTO};
use futures_util::ready;
use futures_util::{stream::empty, Stream, TryStreamExt};
use http::{header::HeaderName, HeaderMap, HeaderValue};
use http_body::Body;
use httparse::{Status, EMPTY_HEADER};
use js_sys::Uint8Array;
use pin_project::pin_project;
use std::ops::{Deref, DerefMut};
use std::{pin::Pin,
          task::{Context, Poll}};
use wasm_bindgen::JsCast;
use wasm_streams::readable::IntoStream;
use web_sys::ReadableStream;

/// If 8th MSB of a frame is `0` for data and `1` for trailer
const TRAILER_BIT: u8 = 0b10000000;

pub struct EncodedBytes {
    content_type: String,
    raw_buf: BytesMut,
    buf: BytesMut,
}

impl EncodedBytes {
    pub fn new(content_type: &str) -> Result<Self, PostGrpcWebErr> {
        Ok(Self {
            content_type: content_type.to_string(),
            raw_buf: BytesMut::new(),
            buf: BytesMut::new(),
        })
    }

    fn is_base64_encoding(&self) -> Result<bool, PostGrpcWebErr> {
        match self.content_type.as_str() {
            APPLICATION_GRPC_WEB_TEXT | APPLICATION_GRPC_WEB_TEXT_PROTO => Ok(true),
            APPLICATION_GRPC_WEB | APPLICATION_GRPC_WEB_PROTO => Ok(false),
            _ => Err(PostGrpcWebErr::InvalidRequest(self.content_type.to_owned())),
        }
    }

    // This is to avoid passing a slice of bytes with a length that the base64
    // decoder would consider invalid.
    #[inline]
    fn max_decodable(&self) -> usize { (self.raw_buf.len() / 4) * 4 }

    fn decode_base64_chunk(&mut self) -> Result<(), PostGrpcWebErr> {
        let index = self.max_decodable();

        if self.raw_buf.len() >= index {
            let decoded = BASE64_STANDARD
                .decode(self.buf.split_to(index))
                .map(Bytes::from)
                .map_err(|err| PostGrpcWebErr::DecodeBody(err.to_string()))?;
            self.buf.put(decoded);
        }

        Ok(())
    }

    fn append(&mut self, bytes: Bytes) -> Result<(), PostGrpcWebErr> {
        if self.is_base64_encoding()? == true {
            self.raw_buf.put(bytes);
            self.decode_base64_chunk()?;
        } else {
            self.buf.put(bytes)
        }

        Ok(())
    }

    fn take(&mut self, length: usize) -> BytesMut {
        let new_buf = self.buf.split_off(length);
        std::mem::replace(&mut self.buf, new_buf)
    }
}

impl Deref for EncodedBytes {
    type Target = BytesMut;

    fn deref(&self) -> &Self::Target { &self.buf }
}

impl DerefMut for EncodedBytes {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.buf }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadState {
    CompressionFlag,
    DataLength,
    Data(u32),
    TrailerLength,
    Trailer(u32),
    Done,
}

impl ReadState {
    fn is_done(&self) -> bool { matches!(self, ReadState::Done) }

    fn finished_data(&self) -> bool {
        matches!(self, ReadState::TrailerLength)
            || matches!(self, ReadState::Trailer(_))
            || matches!(self, ReadState::Done)
    }
}

/// Type to handle HTTP response
#[pin_project]
pub struct ResponseBody {
    #[pin]
    body_stream: BodyStream,
    buf: EncodedBytes,
    incomplete_data: BytesMut,
    data: Option<BytesMut>,
    trailer: Option<HeaderMap>,
    state: ReadState,
    finished_stream: bool,
}

impl ResponseBody {
    pub(crate) fn new(body_stream: ReadableStream, content_type: &str) -> Result<Self, PostGrpcWebErr> {
        let body_stream = wasm_streams::ReadableStream::from_raw(body_stream.unchecked_into()).into_stream();

        Ok(Self {
            body_stream: BodyStream::new(body_stream),
            buf: EncodedBytes::new(content_type)?,
            incomplete_data: BytesMut::new(),
            data: None,
            trailer: None,
            state: ReadState::CompressionFlag,
            finished_stream: false,
        })
    }

    fn read_stream(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), PostGrpcWebErr>> {
        if self.finished_stream {
            return Poll::Ready(Ok(()));
        }

        let this = self.project();

        match ready!(this.body_stream.poll_data(cx)) {
            Some(Ok(data)) => {
                if let Err(e) = this.buf.append(data) {
                    return Poll::Ready(Err(e));
                }

                Poll::Ready(Ok(()))
            },
            Some(Err(e)) => Poll::Ready(Err(e)),
            None => {
                *this.finished_stream = true;
                Poll::Ready(Ok(()))
            },
        }
    }

    fn step(self: Pin<&mut Self>) -> Result<(), PostGrpcWebErr> {
        let this = self.project();

        loop {
            match this.state {
                ReadState::CompressionFlag => {
                    if this.buf.is_empty() {
                        // Can't read compression flag right now
                        return Ok(());
                    } else {
                        let compression_flag = this.buf.take(1);

                        if compression_flag[0] & TRAILER_BIT == 0 {
                            this.incomplete_data.unsplit(compression_flag);
                            *this.state = ReadState::DataLength;
                        } else {
                            *this.state = ReadState::TrailerLength;
                        }
                    }
                },
                ReadState::DataLength => {
                    if this.buf.len() < 4 {
                        // Can't read data length right now
                        return Ok(());
                    } else {
                        let data_length_bytes = this.buf.take(4);
                        let data_length = BigEndian::read_u32(data_length_bytes.as_ref());

                        this.incomplete_data.unsplit(data_length_bytes);
                        *this.state = ReadState::Data(data_length);
                    }
                },
                ReadState::Data(data_length) => {
                    let data_length = *data_length as usize;

                    if this.buf.len() < data_length {
                        // Can't read data right now
                        return Ok(());
                    } else {
                        this.incomplete_data.unsplit(this.buf.take(data_length));

                        let new_data = this.incomplete_data.split();

                        if let Some(data) = this.data {
                            data.unsplit(new_data);
                        } else {
                            *this.data = Some(new_data);
                        }

                        *this.state = ReadState::CompressionFlag;
                    }
                },
                ReadState::TrailerLength => {
                    if this.buf.len() < 4 {
                        // Can't read data length right now
                        return Ok(());
                    } else {
                        let trailer_length_bytes = this.buf.take(4);
                        let trailer_length = BigEndian::read_u32(trailer_length_bytes.as_ref());
                        *this.state = ReadState::Trailer(trailer_length);
                    }
                },
                ReadState::Trailer(trailer_length) => {
                    let trailer_length = *trailer_length as usize;

                    if this.buf.len() < trailer_length {
                        // Can't read trailer right now
                        return Ok(());
                    } else {
                        let mut trailer_bytes = this.buf.take(trailer_length);
                        trailer_bytes.put_u8(b'\n');

                        let mut trailers_buf = [EMPTY_HEADER; 64];
                        let parsed_trailers = match httparse::parse_headers(&trailer_bytes, &mut trailers_buf)
                            .map_err(|err| PostGrpcWebErr::InvalidRequest(err.to_string()))?
                        {
                            Status::Complete((_, headers)) => Ok(headers),
                            Status::Partial => Err(PostGrpcWebErr::InvalidRequest("Invalid".to_string())),
                        }?;

                        let mut trailers = HeaderMap::with_capacity(parsed_trailers.len());

                        for parsed_trailer in parsed_trailers {
                            let header_name = HeaderName::from_bytes(parsed_trailer.name.as_bytes())
                                .map_err(|err| PostGrpcWebErr::InvalidRequest(err.to_string()))?;
                            let header_value = HeaderValue::from_bytes(parsed_trailer.value)
                                .map_err(|err| PostGrpcWebErr::InvalidRequest(err.to_string()))?;
                            trailers.insert(header_name, header_value);
                        }

                        *this.trailer = Some(trailers);

                        *this.state = ReadState::Done;
                    }
                },
                ReadState::Done => return Ok(()),
            }
        }
    }
}

impl Body for ResponseBody {
    type Data = Bytes;

    type Error = PostGrpcWebErr;

    fn poll_data(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        // Check if there's already some data in buffer and return that
        if self.data.is_some() {
            let data = self.data.take().unwrap();
            return Poll::Ready(Some(Ok(data.freeze())));
        }

        // If reading data is finished return `None`
        if self.state.finished_data() {
            return Poll::Ready(None);
        }

        loop {
            // Read bytes from stream
            if let Err(e) = ready!(self.as_mut().read_stream(cx)) {
                return Poll::Ready(Some(Err(e)));
            }

            // Step the state machine
            if let Err(e) = self.as_mut().step() {
                return Poll::Ready(Some(Err(e)));
            }

            if self.data.is_some() {
                // If data is available in buffer, return that
                let data = self.data.take().unwrap();
                return Poll::Ready(Some(Ok(data.freeze())));
            } else if self.state.finished_data() {
                // If we finished reading data continue return `None`
                return Poll::Ready(None);
            } else if self.finished_stream {
                // If stream is finished but data is not finished return error
                return Poll::Ready(Some(Err(PostGrpcWebErr::InvalidRequest("Bad response".to_string()))));
            }
        }
    }

    fn poll_trailers(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        // If the state machine is complete, return trailer
        if self.state.is_done() {
            return Poll::Ready(Ok(self.trailer.take()));
        }

        loop {
            // Read bytes from stream
            if let Err(e) = ready!(self.as_mut().read_stream(cx)) {
                return Poll::Ready(Err(e));
            }

            // Step the state machine
            if let Err(e) = self.as_mut().step() {
                return Poll::Ready(Err(e));
            }

            if self.state.is_done() {
                // If state machine is done, return trailer
                return Poll::Ready(Ok(self.trailer.take()));
            } else if self.finished_stream {
                // If stream is finished but state machine is not done, return error
                return Poll::Ready(Err(PostGrpcWebErr::InvalidRequest("Bad response".to_string())));
            }
        }
    }
}

impl Default for ResponseBody {
    fn default() -> Self {
        Self {
            body_stream: BodyStream::empty(),
            buf: EncodedBytes {
                content_type: Default::default(),
                raw_buf: BytesMut::new(),
                buf: BytesMut::new(),
            },
            incomplete_data: BytesMut::new(),
            data: None,
            trailer: None,
            state: ReadState::Done,
            finished_stream: true,
        }
    }
}

pub struct BodyStream {
    body_stream: Pin<Box<dyn Stream<Item = Result<Bytes, PostGrpcWebErr>>>>,
}

impl BodyStream {
    pub fn new(body_stream: IntoStream<'static>) -> Self {
        let body_stream = body_stream
            .map_ok(|js_value| {
                let buffer = Uint8Array::new(&js_value);

                let mut bytes_vec = vec![0; buffer.length() as usize];
                buffer.copy_to(&mut bytes_vec);

                bytes_vec.into()
            })
            .map_err(|err| PostGrpcWebErr::InvalidRequest(format!("{err:?}")));

        Self {
            body_stream: Box::pin(body_stream),
        }
    }

    pub fn empty() -> Self {
        let body_stream = empty();

        Self {
            body_stream: Box::pin(body_stream),
        }
    }
}

impl Body for BodyStream {
    type Data = Bytes;

    type Error = PostGrpcWebErr;

    fn poll_data(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        self.body_stream.as_mut().poll_next(cx)
    }

    fn poll_trailers(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }
}

unsafe impl Send for BodyStream {}
unsafe impl Sync for BodyStream {}
