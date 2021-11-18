use super::{InsertHeaderMode, MakeHeaderValue};
use http::{header::HeaderName, Response};
use std::{
    fmt,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;
use crate::set_header::{MakeHeaders, MakeFullHeader, And, NoopMakeHeaders, ToMakeHeaders};
use std::future::Future;
use std::pin::Pin;
use pin_project::pin_project;
use futures_util::ready;
use std::marker::PhantomData;

pub struct SetMultipleResponseHeadersLayer<M> {
    make_headers: M,
}


impl<M> fmt::Debug for SetMultipleResponseHeadersLayer<M> {
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

impl<M> SetMultipleResponseHeadersLayer<M> {
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

impl<S, M> Layer<S> for SetMultipleResponseHeadersLayer<M>
    where
        M: MakeHeaders<()> + Clone,
{
    type Service = SetMultipleResponseHeaders<S, M, ()>;

    fn layer(&self, inner: S) -> Self::Service {
        SetMultipleResponseHeaders {
            inner,
            make: self.make_headers.clone(),
            _marker: PhantomData::default(),
        }
    }
}

impl<M> Clone for SetMultipleResponseHeadersLayer<M>
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
pub struct SetMultipleResponseHeaders<S, M: MakeHeaders<T>, T> {
    inner: S,
    make: M,
    _marker: PhantomData<T>,
}

impl<S, T> SetMultipleResponseHeaders<S, NoopMakeHeaders, T> {
    fn new(inner: S) -> SetMultipleResponseHeaders<S, NoopMakeHeaders, T> {
        SetMultipleResponseHeaders {
            inner,
            make: NoopMakeHeaders { },
            _marker: PhantomData::default()
        }

    }
}

impl<S, M: MakeHeaders<T>, T> SetMultipleResponseHeaders<S, M, T> {

    pub fn appending<Mhv: MakeHeaderValue<T> + Clone>(self, header_name: HeaderName, make: Mhv) -> SetMultipleResponseHeaders<S, And<ToMakeHeaders<Mhv, T>, M>, T> {
        self.add_make_headers(header_name, make, InsertHeaderMode::Append)
    }

    pub fn overriding<Mhv: MakeHeaderValue<T> + Clone>(self, header_name: HeaderName, make: Mhv) -> SetMultipleResponseHeaders<S, And<ToMakeHeaders<Mhv, T>, M>, T> {
        self.add_make_headers(header_name, make, InsertHeaderMode::Override)
    }

    pub fn if_not_present<Mhv: MakeHeaderValue<T> + Clone>(self, header_name: HeaderName, make: Mhv) -> SetMultipleResponseHeaders<S, And<ToMakeHeaders<Mhv, T>, M>, T> {
        self.add_make_headers(header_name, make, InsertHeaderMode::IfNotPresent)
    }

    fn add_make_headers<Mhv: MakeHeaderValue<T> + Clone>(self, header_name: HeaderName, make: Mhv, mode: InsertHeaderMode) -> SetMultipleResponseHeaders<S, And<ToMakeHeaders<Mhv, T>, M>, T> {
        SetMultipleResponseHeaders {
            inner: self.inner,
            make: ToMakeHeaders {
                _marker: PhantomData::default(),
                header_name,
                mode,
                make
            }.and(self.make),
            _marker: Default::default()
        }
    }

    pub fn custom<Mk: MakeFullHeader<T> + Clone>(self, make: Mk) -> SetMultipleResponseHeaders<S, And<Mk, M>, T> {
        SetMultipleResponseHeaders {
            inner: self.inner,
            make: make.and(self.make),
            _marker: Default::default()
        }
    }

    define_inner_service_accessors!();
}

impl<S, M, T> fmt::Debug for SetMultipleResponseHeaders<S, M, T>
    where
        S: fmt::Debug,
        M: MakeHeaders<T> + Clone
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetRequestHeader")
            .field("inner", &self.inner)
            .field("make", &std::any::type_name::<M>())
            .finish()
    }
}

impl<Req, ResBody, S, M> Service<Req> for SetMultipleResponseHeaders<S, M, Response<ResBody>>
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

        let headers = this.make.make_headers(&mut res);
        for header in headers {
            header.mode.apply(&header.name, &mut res, header.value);
        }
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
    use crate::set_header::PreparedHeader;

    #[tokio::test]
    async fn test_composing_headers() {
        let custom = |_res: &Response<Body>| {
            PreparedHeader {
                name: header::ACCEPT_CHARSET,
                value: Some(HeaderValue::from_static("utf8")),
                mode: InsertHeaderMode::IfNotPresent
            }
        };
        let svc = SetMultipleResponseHeaders::new(
            service_fn(|_req: ()| async {
                let res = Response::builder()
                    .header(header::CONTENT_TYPE, "good-content")
                    .header(header::CONTENT_LENGTH, "555")
                    .body(Body::empty())
                    .unwrap();
                Ok::<_, Infallible>(res)
            }),
        )
            .overriding(header::CONTENT_TYPE, HeaderValue::from_static("text/html"))
            .appending(header::CONTENT_LENGTH, HeaderValue::from_static("abc"))
            .if_not_present(header::CONTENT_TYPE, HeaderValue::from_static("111"))
            .custom(custom);

        let res = svc.oneshot(()).await.unwrap();

        let mut values = res.headers().get_all(header::CONTENT_TYPE).iter();
        assert_eq!(values.next().unwrap(), "text/html");
        assert_eq!(values.next(), None);
        values = res.headers().get_all(header::CONTENT_LENGTH).iter();
        assert_eq!(values.next().unwrap(), "555");
        assert_eq!(values.next().unwrap(), "abc");
        assert_eq!(values.next(), None);
        values = res.headers().get_all(header::ACCEPT_CHARSET).iter();
        assert_eq!(values.next().unwrap(), "utf8");
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
