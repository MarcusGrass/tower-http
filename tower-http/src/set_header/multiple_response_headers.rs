use super::{InsertHeaderMode, MakeHeaderValue};
use http::{header::HeaderName, Request, HeaderValue, Response};
use std::{
    fmt,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;
use crate::set_header::{MakeHeaders, MakeFullHeader, And, EmptyMakeHeaders, NoopHeaders};
use std::future::Future;
use std::pin::Pin;
use pin_project::pin_project;
use futures_util::ready;
use std::marker::PhantomData;

pub struct SetManyResponseHeadersLayer<M> {
    make_headers: M,
}

#[derive(Clone, Debug)]
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
        M: MakeHeaders<()> + Clone,
{
    type Service = SetResponseHeader<S, M, ()>;

    fn layer(&self, inner: S) -> Self::Service {
        SetResponseHeader {
            inner,
            make: self.make_headers.clone(),
            _marker: PhantomData::default(),
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
pub struct SetResponseHeader<S, M: MakeHeaders<T>, T> {
    inner: S,
    make: M,
    _marker: PhantomData<T>,
}

impl<S, T> SetResponseHeader<S, NoopHeaders<T>, T> {
    fn new(inner: S) -> SetResponseHeader<S, NoopHeaders<T>, T> {
        SetResponseHeader {
            inner,
            make: NoopHeaders{ _marker: Default::default() },
            _marker: PhantomData::default()
        }

    }
}

pub struct ToMakeHeaders<M, T> where M: MakeHeaderValue<T> + Clone {
    _marker: PhantomData<T>,
    header_name: HeaderName,
    mode: InsertHeaderMode,
    make: M
}

impl<M, T> Clone for ToMakeHeaders<M, T> where M: MakeHeaderValue<T> + Clone {
    fn clone(&self) -> Self {
        Self {
            _marker: self._marker,
            header_name: self.header_name.clone(),
            mode: self.mode,
            make: self.make.clone()
        }
    }
}

impl<M, T> MakeFullHeader<T> for ToMakeHeaders<M, T> where M: MakeHeaderValue<T> + Clone {
    fn make_full_header(&mut self, message: &T) -> PreparedHeader {
        PreparedHeader {
            name: self.header_name.clone(),
            value: self.make.make_header_value(message),
            mode: self.mode
        }
    }
}

impl<S, M: MakeHeaders<T>, T> SetResponseHeader<S, M, T> {

    pub fn appending<Mhv: MakeHeaderValue<T> + Clone>(self, header_name: HeaderName, make: Mhv) -> SetResponseHeader<S, And<ToMakeHeaders<Mhv, T>, M>, T> {
        SetResponseHeader {
            inner: self.inner,
            make: ToMakeHeaders {
                _marker: PhantomData::default(),
                header_name,
                mode: InsertHeaderMode::Append,
                make
            }.and(self.make),
            _marker: Default::default()
        }
    }

    pub fn overriding<Mhv: MakeHeaderValue<T> + Clone>(self, header_name: HeaderName, make: Mhv) -> SetResponseHeader<S, And<ToMakeHeaders<Mhv, T>, M>, T> {
        SetResponseHeader {
            inner: self.inner,
            make: ToMakeHeaders {
                _marker: PhantomData::default(),
                header_name,
                mode: InsertHeaderMode::Override,
                make
            }.and(self.make),
            _marker: Default::default()
        }
    }

    pub fn if_not_present<Mhv: MakeHeaderValue<T> + Clone>(self, header_name: HeaderName, make: Mhv) -> SetResponseHeader<S, And<ToMakeHeaders<Mhv, T>, M>, T> {
        SetResponseHeader {
            inner: self.inner,
            make: ToMakeHeaders {
                _marker: PhantomData::default(),
                header_name,
                mode: InsertHeaderMode::IfNotPresent,
                make
            }.and(self.make),
            _marker: Default::default()
        }
    }

    pub fn custom<Mk: MakeFullHeader<T> + Clone>(self, make: Mk) -> SetResponseHeader<S, And<Mk, M>, T> {
        SetResponseHeader {
            inner: self.inner,
            make: make.and(self.make),
            _marker: Default::default()
        }
    }

    define_inner_service_accessors!();
}

impl<S, M, T> fmt::Debug for SetResponseHeader<S, M, T>
    where
        S: fmt::Debug,
        M: MakeHeaders<T> + Clone
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

impl<Req, ResBody, S, M> Service<Req> for SetResponseHeader<S, M, Response<ResBody>>
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
            if let Some(value) = header.value {
                header.mode.apply_prepared(&header.name, &mut res, value)
            }
        }
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
    async fn test_composing_headers() {
        let custom = |_res: &Response<Body>| {
            PreparedHeader {
                name: header::ACCEPT_CHARSET,
                value: Some(HeaderValue::from_static("utf8")),
                mode: InsertHeaderMode::IfNotPresent
            }
        };
        let svc = SetResponseHeader::new(
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
