//! Middleware for setting headers on requests and responses.
//!
//! See [request] and [response] for more details.

use http::{header::HeaderName, HeaderMap, HeaderValue, Request, Response};

pub mod request;
pub mod response;
pub mod multiple_response_headers;

#[doc(inline)]
pub use self::{
    request::{SetRequestHeader, SetRequestHeaderLayer},
    response::{SetResponseHeader, SetResponseHeaderLayer},
};
use std::marker::PhantomData;

/// Trait for producing header values.
///
/// Used by [`SetRequestHeader`] and [`SetResponseHeader`].
///
/// This trait is implemented for closures with the correct type signature. Typically users will
/// not have to implement this trait for their own types.
///
/// It is also implemented directly for [`HeaderValue`]. When a fixed header value should be added
/// to all responses, it can be supplied directly to the middleware.
pub trait MakeHeaderValue<T> {
    /// Try to create a header value from the request or response.
    fn make_header_value(&mut self, message: &T) -> Option<HeaderValue>;
}

impl<F, T> MakeHeaderValue<T> for F
where
    F: FnMut(&T) -> Option<HeaderValue>,
{
    fn make_header_value(&mut self, message: &T) -> Option<HeaderValue> {
        self(message)
    }
}

impl<T> MakeHeaderValue<T> for HeaderValue {
    fn make_header_value(&mut self, _message: &T) -> Option<HeaderValue> {
        Some(self.clone())
    }
}

impl<T> MakeHeaderValue<T> for Option<HeaderValue> {
    fn make_header_value(&mut self, _message: &T) -> Option<HeaderValue> {
        self.clone()
    }
}


#[derive(Clone, Debug)]
pub struct PreparedHeader {
    name: HeaderName,
    value: Option<HeaderValue>,
    mode: InsertHeaderMode,
}

impl PreparedHeader {
    fn new(name: HeaderName, value: Option<HeaderValue>, mode: InsertHeaderMode) -> Self {
        PreparedHeader {
            name,
            value,
            mode
        }
    }
    pub fn if_not_present(name: HeaderName, value: Option<HeaderValue>) -> Self {
        Self::new(name, value, InsertHeaderMode::IfNotPresent)
    }
    pub fn overriding(name: HeaderName, value: Option<HeaderValue>) -> Self {
        Self::new(name, value, InsertHeaderMode::Override)
    }
    pub fn appending(name: HeaderName, value: Option<HeaderValue>) -> Self {
        Self::new(name, value, InsertHeaderMode::Append)
    }
}

pub trait MakeHeaders<T> {
    fn make_headers(&mut self, message: &T) -> Vec<PreparedHeader>;
}


pub trait MakeFullHeader<T> {
    fn make_full_header(&mut self, message: &T) -> PreparedHeader;

    fn and<Other>(self, other: Other) -> And<Self, Other>
    where
        Self: Sized,
        Other: MakeHeaders<T>
    {
        And {
            left: self,
            right: other,
        }
    }
}

#[derive(Copy, Clone)]
pub struct NoopMakeHeaders {
}

impl<T> MakeHeaders<T> for NoopMakeHeaders {
    fn make_headers(&mut self, _message: &T) -> Vec<PreparedHeader> {
        vec![]
    }
}

#[derive(Clone)]
pub struct And<Left, Right> {
    left: Left,
    right: Right,
}

impl<Left, Right, T> MakeHeaders<T> for And<Left, Right> where Left: MakeFullHeader<T>, Right: MakeHeaders<T>{
    fn make_headers(&mut self, message: &T) -> Vec<PreparedHeader> {
        let mut all = self.right.make_headers(message);
        all.push(self.left.make_full_header(message));
        all
    }
}

impl<T, F> MakeFullHeader<T> for F where F: Fn(&T) -> PreparedHeader {
    fn make_full_header(&mut self, message: &T) -> PreparedHeader {
        (self)(message)
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
        PreparedHeader::new(self.header_name.clone(), self.make.make_header_value(message), self.mode)
    }
}


#[derive(Debug, Clone, Copy)]
enum InsertHeaderMode {
    Override,
    Append,
    IfNotPresent,
}

impl InsertHeaderMode {
    fn apply<T>(self, header_name: &HeaderName, target: &mut T, header_value: Option<HeaderValue>)
    where
        T: Headers,
    {
        if let Some(value) = header_value {
            match self {
                InsertHeaderMode::Override => {
                    target.headers_mut().insert(header_name.clone(), value);
                }
                InsertHeaderMode::IfNotPresent => {
                    if !target.headers().contains_key(header_name) {
                        target.headers_mut().insert(header_name.clone(), value);
                    }
                }
                InsertHeaderMode::Append => {
                    target.headers_mut().append(header_name.clone(), value);
                }
            }
        }

    }
}

trait Headers {
    fn headers(&self) -> &HeaderMap;

    fn headers_mut(&mut self) -> &mut HeaderMap;
}

impl<B> Headers for Request<B> {
    fn headers(&self) -> &HeaderMap {
        Request::headers(self)
    }

    fn headers_mut(&mut self) -> &mut HeaderMap {
        Request::headers_mut(self)
    }
}

impl<B> Headers for Response<B> {
    fn headers(&self) -> &HeaderMap {
        Response::headers(self)
    }

    fn headers_mut(&mut self) -> &mut HeaderMap {
        Response::headers_mut(self)
    }
}
