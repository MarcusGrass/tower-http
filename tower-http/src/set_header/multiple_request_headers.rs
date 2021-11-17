use super::{InsertHeaderMode, MakeHeaderValue};
use http::{header::HeaderName, Request, HeaderValue};
use std::{
    fmt,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;
use crate::set_header::ComposeMakeHeaders;

pub struct SetManyRequestHeadersLayer<M> {
    make_headers: M,
}

#[derive(Clone)]
pub struct PreparedHeader {
    name: HeaderName,
    pub(crate) value: Option<HeaderValue>,
    mode: InsertHeaderMode,
}

impl<M> fmt::Debug for SetManyRequestHeadersLayer<M> {
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

impl<M> SetManyRequestHeadersLayer<M> {
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

impl<S, M> Layer<S> for SetManyRequestHeadersLayer<M>
    where
        M: Clone,
{
    type Service = SetRequestHeader<S, M>;

    fn layer(&self, inner: S) -> Self::Service {
        SetRequestHeader {
            inner,
            make: self.make_headers.clone(),
        }
    }
}

impl<M> Clone for SetManyRequestHeadersLayer<M>
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
pub struct SetRequestHeader<S, M> {
    inner: S,
    make: M,
}

impl<S, M> SetRequestHeader<S, M> {
    /// Create a new [`SetRequestHeader`].
    ///
    /// If a previous value exists for the same header, it is removed and replaced with the new
    /// header value.
    pub fn overriding(inner: S, header_name: HeaderName, make: M) -> Self {
        Self::new(inner, header_name, make, InsertHeaderMode::Override)
    }

    /// Create a new [`SetRequestHeader`].
    ///
    /// The new header is always added, preserving any existing values. If previous values exist,
    /// the header will have multiple values.
    pub fn appending(inner: S, header_name: HeaderName, make: M) -> Self {
        Self::new(inner, header_name, make, InsertHeaderMode::Append)
    }

    /// Create a new [`SetRequestHeader`].
    ///
    /// If a previous value exists for the header, the new value is not inserted.
    pub fn if_not_present(inner: S, header_name: HeaderName, make: M) -> Self {
        Self::new(inner, header_name, make, InsertHeaderMode::IfNotPresent)
    }

    fn new(inner: S, header_name: HeaderName, make: M, mode: InsertHeaderMode) -> Self {
        Self {
            inner,
            make,
        }
    }

    define_inner_service_accessors!();
}

impl<S, M> fmt::Debug for SetRequestHeader<S, M>
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

impl<ReqBody, S, M> Service<Request<ReqBody>> for SetRequestHeader<S, M>
    where
        S: Service<Request<ReqBody>>,
        M: MakeHeaderValue<Request<ReqBody>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        //self.mode.apply(&self.header_name, &mut req, &mut self.make);
        self.inner.call(req)
    }
}
