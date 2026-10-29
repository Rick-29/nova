//! The delegating method shared by every backend:
//!
//! ```text
//! <attrs>
//! pub fn name(&mut self, arg: Foreign) -> Result<Foreign, BackendError> {
//!     <lock>
//!     let __value = <Indicator<f64> as Trait>::method(&mut self.inner, lift(arg))?;
//!     Ok(lower(__value))
//! }
//! ```
//!
//! Backends supply the boundary types, the error mapping and the attributes.

use syn::{Ident, Type};
use zyn::TokenStream;

use crate::backends::doc_attrs;
use crate::backends::types::{Direction, ScalarMapping, foreign_type, lift, lower};
use crate::codegen::delegate::{call, unwrap_or_return};
use crate::ir::{BindingType, IndicatorData, MethodData, MethodKind, Receiver};

/// How the wrapper reaches the indicator.
pub enum Access {
    /// `self.inner` is the indicator (`&mut self` methods).
    Direct,
    /// `self.inner` is a `Mutex` (`&self` methods on shared objects).
    Mutex,
}

pub struct WrapperFn<'a> {
    pub map: &'a dyn ScalarMapping,
    pub numeric: Type,
    pub access: Access,
    /// Name of the generated Rust function.
    pub name: Ident,
    /// Backend attributes placed after the docs, e.g. `#[new]`.
    pub attrs: TokenStream,
    /// Maps the configured error value into the backend's error type.
    pub map_error: &'a dyn Fn(TokenStream) -> TokenStream,
    /// `Result<T, BackendError>` for a success type `T`.
    pub result_type: &'a dyn Fn(TokenStream) -> TokenStream,
    /// Constructor success type and value built from `__value`.
    pub construct: (TokenStream, TokenStream),
}

impl WrapperFn<'_> {
    pub fn build(&self, data: &IndicatorData, method: &MethodData) -> TokenStream {
        let map = self.map;
        let numeric = &self.numeric;
        let docs = doc_attrs(&method.docs);
        let attrs = &self.attrs;
        let name = &self.name;

        let params: Vec<TokenStream> = method
            .args
            .iter()
            .map(|arg| {
                let name = &arg.name;
                let ty = foreign_type(map, &arg.ty, numeric, Direction::In);
                template! { {{ name }}: {{ ty }} }
            })
            .collect();
        let lifted: Vec<TokenStream> = method
            .args
            .iter()
            .map(|arg| {
                let name = &arg.name;
                lift(map, &arg.ty, template! { {{ name }} })
            })
            .collect();

        let mutex = matches!(self.access, Access::Mutex);
        let (self_param, lock, receiver) = match (&method.receiver, mutex) {
            (Receiver::None, _) => (TokenStream::new(), TokenStream::new(), None),
            (Receiver::Ref, false) => (
                template! { &self, },
                TokenStream::new(),
                Some(template! { &self.inner }),
            ),
            (Receiver::RefMut, false) => (
                template! { &mut self, },
                TokenStream::new(),
                Some(template! { &mut self.inner }),
            ),
            (Receiver::Ref, true) => (
                template! { &self, },
                template! { let __inner = self.inner.lock().unwrap_or_else(::std::sync::PoisonError::into_inner); },
                Some(template! { &*__inner }),
            ),
            (Receiver::RefMut, true) => (
                template! { &self, },
                template! { let mut __inner = self.inner.lock().unwrap_or_else(::std::sync::PoisonError::into_inner); },
                Some(template! { &mut *__inner }),
            ),
        };

        let fallible = method.output.fallible;
        let value = unwrap_or_return(
            call(method, numeric, receiver, &lifted),
            fallible,
            &data.config.error_type,
            self.map_error,
        );

        let (ok_type, result) = if method.kind == MethodKind::Constructor {
            self.construct.clone()
        } else if matches!(method.output.ok, BindingType::Unit) {
            (template! { () }, template! { () })
        } else {
            (
                foreign_type(map, &method.output.ok, numeric, Direction::Out),
                lower(map, &method.output.ok, template! { __value }),
            )
        };
        let unit =
            matches!(method.output.ok, BindingType::Unit) && method.kind != MethodKind::Constructor;
        let (return_type, body) = match (fallible, unit) {
            (true, _) => (
                (self.result_type)(ok_type),
                template! {
                    let __value = {{ value }};
                    ::core::result::Result::Ok({{ result }})
                },
            ),
            (false, true) => (ok_type, template! { {{ value }}; }),
            (false, false) => (
                ok_type,
                template! {
                    let __value = {{ value }};
                    {{ result }}
                },
            ),
        };

        template! {
            {{ docs }}
            {{ attrs }}
            pub fn {{ name }}({{ self_param }} @for (param in params.iter()) { {{ param }}, }) -> {{ return_type }} {
                {{ lock }}
                {{ body }}
            }
        }
    }
}
