use super::{InsertHeaderMode, MakeHeaderValue};
use http::{header::HeaderName, Request, HeaderValue, Response};
use std::{
    fmt,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;
use crate::set_header::{ComposeMakeHeaders, MakeHeaders};
use std::future::Future;
use std::pin::Pin;
use pin_project::pin_project;
use futures_util::ready;

pub struct SetManyResponseHeadersLayer<M> {
    make_headers: M,
}

#[derive(Clone)]
pub struct PreparedHeader {
    name: HeaderName,
    pub(crate) value: Option<HeaderValue>,
    mode: InsertHeaderMode,
}

impl<M> fmt::Debug for SetManyResponseHeadersLayer<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /*
        f.debug_struct("SetRequestHeaderLayer")
            .field("header_name", &self.header_name)
            .field("mode", &self.mode)
            .field("make", &std::any::type_name::<M>())
            .finish()

         */
        f.write_str("")
    }
}

impl<M> SetManyResponseHeadersLayer<M> {
    /// Create a new [`SetRequestHeaderLayer`].
    ///
    /// If a previous value exists for the same header, it is removed and replaced with the new
    /// header value.
    pub fn overriding(make: M) -> Self {
        Self::new(make)
    }

    /// Create a new [`SetRequestHeaderLayer`].
    ///
    /// The new header is always added, preserving any existing values. If previous values exist,
    /// the header will have multiple values.
    pub fn appending(make: M) -> Self {
        Self::new(make)
    }

    /// Create a new [`SetRequestHeaderLayer`].
    ///
    /// If a previous value exists for the header, the new value is not inserted.
    pub fn if_not_present(make: M) -> Self {
        Self::new(make)
    }

    fn new(make: M) -> Self {
        Self {
            make_headers: make,
        }
    }
}

impl<S, M> Layer<S> for SetManyResponseHeadersLayer<M>
    where
        M: Clone,
{
    type Service = SetResponseHeader<S, M>;

    fn layer(&self, inner: S) -> Self::Service {
        SetResponseHeader {
            inner,
            make: self.make_headers.clone(),
        }
    }
}

impl<M> Clone for SetManyResponseHeadersLayer<M>
    where
        M: Clone,
{
    fn clone(&self) -> Self {
        Self {
            make_headers: self.make_headers.clone()
        }
    }
}

/// Middleware that sets a header on the request.
#[derive(Clone)]
pub struct SetResponseHeader<S, M> {
    inner: S,
    make: M,
}

impl<S, M> SetResponseHeader<S, M> {

    fn new(inner: S, make: M) -> Self {
        Self {
            inner,
            make,
        }
    }

    define_inner_service_accessors!();
}

impl<S, M> fmt::Debug for SetResponseHeader<S, M>
    where
        S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /*
        f.debug_struct("SetRequestHeader")
            .field("inner", &self.inner)
            .field("header_name", &self.header_name)
            .field("mode", &self.mode)
            .field("make", &std::any::type_name::<M>())
            .finish()

         */
        f.write_str("")
    }
}

impl<Req, ResBody, S, M> Service<Req> for SetResponseHeader<S, M>
    where
        S: Service<Req, Response = Response<ResBody>>,
        M: MakeHeaders<Response<ResBody>> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, M>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        ResponseFuture {
            future: self.inner.call(req),
            make: self.make.clone(),
        }
    }
}


/// Response future for [`SetResponseHeader`].
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<F, M> {
    #[pin]
    future: F,
    make: M,
}

impl<F, ResBody, E, M> Future for ResponseFuture<F, M>
    where
        F: Future<Output = Result<Response<ResBody>, E>>,
        M: MakeHeaders<Response<ResBody>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let mut res = ready!(this.future.poll(cx)?);

        //this.make.(this.header_name, &mut res, &mut *this.make);

        Poll::Ready(Ok(res))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use http::{header, HeaderValue};
    use hyper::Body;
    use std::convert::Infallible;
    use tower::{service_fn, ServiceExt};

    #[tokio::test]
    async fn test_override_mode() {
        let svc = SetResponseHeader::new(
            service_fn(|_req: ()| async {
                let res = Response::builder()
                    .header(header::CONTENT_TYPE, "good-content")
                    .body(Body::empty())
                    .unwrap();
                Ok::<_, Infallible>(res)
            }),
            |res: &Response<Body>| {
                vec![PreparedHeader {
                    name: header::CONTENT_TYPE,
                    value: Some(HeaderValue::from_static("text/html")),
                    mode: InsertHeaderMode::Override
                }]

            }
        );

        let res = svc.oneshot(()).await.unwrap();

        let mut values = res.headers().get_all(header::CONTENT_TYPE).iter();
        assert_eq!(values.next().unwrap(), "text/html");
        assert_eq!(values.next(), None);
    }

    /*
    #[tokio::test]
    async fn test_append_mode() {
        let svc = SetResponseHeader::appending(
            service_fn(|_req: ()| async {
                let res = Response::builder()
                    .header(header::CONTENT_TYPE, "good-content")
                    .body(Body::empty())
                    .unwrap();
                Ok::<_, Infallible>(res)
            }),
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html"),
        );

        let res = svc.oneshot(()).await.unwrap();

        let mut values = res.headers().get_all(header::CONTENT_TYPE).iter();
        assert_eq!(values.next().unwrap(), "good-content");
        assert_eq!(values.next().unwrap(), "text/html");
        assert_eq!(values.next(), None);
    }

    #[tokio::test]
    async fn test_skip_if_present_mode() {
        let svc = SetResponseHeader::if_not_present(
            service_fn(|_req: ()| async {
                let res = Response::builder()
                    .header(header::CONTENT_TYPE, "good-content")
                    .body(Body::empty())
                    .unwrap();
                Ok::<_, Infallible>(res)
            }),
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html"),
        );

        let res = svc.oneshot(()).await.unwrap();

        let mut values = res.headers().get_all(header::CONTENT_TYPE).iter();
        assert_eq!(values.next().unwrap(), "good-content");
        assert_eq!(values.next(), None);
    }

    #[tokio::test]
    async fn test_skip_if_present_mode_when_not_present() {
        let svc = SetResponseHeader::if_not_present(
            service_fn(|_req: ()| async {
                let res = Response::builder().body(Body::empty()).unwrap();
                Ok::<_, Infallible>(res)
            }),
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html"),
        );

        let res = svc.oneshot(()).await.unwrap();

        let mut values = res.headers().get_all(header::CONTENT_TYPE).iter();
        assert_eq!(values.next().unwrap(), "text/html");
        assert_eq!(values.next(), None);
    }

     */
}
