//! The static-or-dynamic value abstraction.
//!
//! Every component property that can reasonably be resolved at render time
//! accepts either its ordinary value or an asynchronous callback returning
//! that value, with no separate constructors ([`Value`]). Fixed component
//! slots additionally accept absence ([`Slot`]).

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use crate::context::RenderCtx;
use crate::error::Result;

/// A pinned boxed future, used to keep callback traits object-safe.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A property value that is either static or resolved at render time.
///
/// `V` is the property's value type, `T` the Page's data type and `E` the
/// database extension type.
pub enum Value<V, T, E> {
    Static(V),
    Dynamic(
        Box<dyn for<'a> Fn(&'a RenderCtx<'a, T, E>, T) -> BoxFuture<'a, Result<V>> + Send + Sync>,
    ),
}

impl<V: std::fmt::Debug, T, E> std::fmt::Debug for Value<V, T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static(v) => f.debug_tuple("Static").field(v).finish(),
            Self::Dynamic(_) => f.write_str("Dynamic(..)"),
        }
    }
}

impl<V: Clone, T: Clone, E> Value<V, T, E> {
    /// Resolve the value at render time.
    pub fn resolve<'a>(
        &'a self,
        ctx: &'a RenderCtx<'a, T, E>,
        data: &'a T,
    ) -> BoxFuture<'a, Result<V>>
    where
        V: Send + Sync,
        T: Send + Sync,
        E: Sync,
    {
        match self {
            Self::Static(v) => Box::pin(async move { Ok(v.clone()) }),
            Self::Dynamic(f) => {
                let data = data.clone();
                f(ctx, data)
            }
        }
    }
}

/// Wrapper marking a closure as a dynamic value callback.
///
/// Construct with [`dynamic`].
pub struct DynFn<V, T, E, F>(pub F, pub PhantomData<fn() -> (V, T, E)>);

/// Mark a closure as a dynamic (render-time resolved) property value.
///
/// The callback receives the render context and a clone of the Page's data,
/// and returns a boxed async block:
///
/// ```ignore
/// .add_text(dynamic(|ctx, data| Box::pin(async move {
///     Ok(format!("Level: {}", data.level))
/// })))
/// ```
pub fn dynamic<V, T, E, F>(f: F) -> DynFn<V, T, E, F>
where
    V: Send + 'static,
    T: Send + Sync + 'static,
    F: for<'a> Fn(&'a RenderCtx<'a, T, E>, T) -> BoxFuture<'a, Result<V>> + Send + Sync + 'static,
{
    DynFn(f, PhantomData)
}

impl<V: Send + 'static, T: Send + Sync + 'static, E, F> From<DynFn<V, T, E, F>> for Value<V, T, E>
where
    F: for<'a> Fn(&'a RenderCtx<'a, T, E>, T) -> BoxFuture<'a, Result<V>> + Send + Sync + 'static,
{
    fn from(v: DynFn<V, T, E, F>) -> Self {
        Value::Dynamic(Box::new(v.0))
    }
}

// Static conversions for the ordinary property types.
impl<T: Send + Sync + 'static, E> From<&str> for Value<String, T, E> {
    fn from(v: &str) -> Self {
        Value::Static(v.to_string())
    }
}

impl<T: Send + Sync + 'static, E> From<String> for Value<String, T, E> {
    fn from(v: String) -> Self {
        Value::Static(v)
    }
}

impl<T: Send + Sync + 'static, E> From<bool> for Value<bool, T, E> {
    fn from(v: bool) -> Self {
        Value::Static(v)
    }
}

impl<T: Send + Sync + 'static, E> From<u32> for Value<u32, T, E> {
    fn from(v: u32) -> Self {
        Value::Static(v)
    }
}

impl<T: Send + Sync + 'static, E> From<serenity::model::application::ButtonStyle>
    for Value<serenity::model::application::ButtonStyle, T, E>
{
    fn from(v: serenity::model::application::ButtonStyle) -> Self {
        Value::Static(v)
    }
}

impl SlotValue for serenity::model::application::ButtonStyle {}
impl SlotValue for bool {}
impl SlotValue for u32 {}

/// One of the fixed component slots: a static value, a dynamic callback, or
/// nothing (`None`).
#[derive(Default)]
pub enum Slot<V, T, E> {
    #[default]
    Empty,
    Val(Value<V, T, E>),
}

impl<V: std::fmt::Debug, T, E> std::fmt::Debug for Slot<V, T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("Empty"),
            Self::Val(v) => f.debug_tuple("Val").field(v).finish(),
        }
    }
}

impl<V: Clone, T: Clone, E> Slot<V, T, E> {
    /// Resolve the slot; `None` when the slot is empty.
    pub fn resolve<'a>(
        &'a self,
        ctx: &'a RenderCtx<'a, T, E>,
        data: &'a T,
    ) -> BoxFuture<'a, Result<Option<V>>>
    where
        V: Send + Sync,
        T: Send + Sync + 'static,
        E: Sync + Send + 'static,
    {
        match self {
            Self::Empty => Box::pin(async move { Ok(None) }),
            Self::Val(v) => Box::pin(async move { Ok(Some(v.resolve(ctx, data).await?)) }),
        }
    }
}

/// Marker for types usable directly as fixed-slot values.
pub trait SlotValue: Send + Sync + 'static {}

impl SlotValue for String {}

impl<V: SlotValue, T: Send + Sync + 'static, E> From<V> for Slot<V, T, E> {
    fn from(v: V) -> Self {
        Slot::Val(Value::Static(v))
    }
}

impl<V: Send + Sync + 'static, T: Send + Sync + 'static, E> From<Option<V>> for Slot<V, T, E> {
    fn from(v: Option<V>) -> Self {
        match v {
            Some(v) => Slot::Val(Value::Static(v)),
            None => Slot::Empty,
        }
    }
}

impl<V: Send + 'static, T: Send + Sync + 'static, E, F> From<DynFn<V, T, E, F>> for Slot<V, T, E>
where
    F: for<'a> Fn(&'a RenderCtx<'a, T, E>, T) -> BoxFuture<'a, Result<V>> + Send + Sync + 'static,
{
    fn from(v: DynFn<V, T, E, F>) -> Self {
        Slot::Val(Value::Dynamic(Box::new(v.0)))
    }
}
