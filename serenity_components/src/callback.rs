//! Execution callbacks: the explicit half of the static/dynamic split.
//!
//! A dynamic value determines what a component looks like; an execution
//! determines what happens when someone interacts with a component. The
//! latter is always an explicit callback.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use serenity::model::application::CommandDataResolved;
use serenity::model::channel::GenericInteractionChannel;
use serenity::model::id::{AttachmentId, GenericChannelId, GenericId, RoleId, UserId};
use serenity::model::user::User;

use crate::component::Element;
use crate::context::Ctx;
use crate::error::{Error, Result};
use crate::modal::ModalSubmission;
use crate::value::BoxFuture;

/// The selected values of a selection menu execution, plus the entities
/// Discord resolved alongside them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection<S> {
    /// The selected values.
    #[serde(default)]
    pub values: S,
    /// Entities resolved by Discord for the selection.
    #[serde(default)]
    pub resolved: CommandDataResolved,
}

/// A selected user with its resolved record, if Discord provided one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserChoice {
    /// The selected user ID.
    pub id: UserId,
    /// The resolved user record.
    pub user: Option<User>,
}

/// A selected role with its resolved record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleChoice {
    /// The selected role ID.
    pub id: RoleId,
    /// The resolved role record.
    pub role: Option<serenity::model::guild::Role>,
}

/// A selected channel with its resolved record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelChoice {
    /// The selected channel ID.
    pub id: GenericChannelId,
    /// The resolved channel record.
    pub channel: Option<GenericInteractionChannel>,
}

/// A selected user-or-role from a Mentionable Select.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionableChoice {
    /// The selected ID.
    pub id: GenericId,
    /// The resolved user record, if the selection was a user.
    pub user: Option<User>,
    /// The resolved role record, if the selection was a role.
    pub role: Option<serenity::model::guild::Role>,
}

/// The selection value type for each select kind.
pub trait SelectValues: Send + Sync + 'static {
    /// The kind name for diagnostics.
    const KIND: &'static str;
}

impl SelectValues for () {
    const KIND: &'static str = "none";
}
impl SelectValues for Vec<String> {
    const KIND: &'static str = "string";
}
impl SelectValues for Vec<UserChoice> {
    const KIND: &'static str = "user";
}
impl SelectValues for Vec<RoleChoice> {
    const KIND: &'static str = "role";
}
impl SelectValues for Vec<ChannelChoice> {
    const KIND: &'static str = "channel";
}
impl SelectValues for Vec<MentionableChoice> {
    const KIND: &'static str = "mentionable";
}

/// An execution: the callback invoked when a component is interacted with.
///
/// Construct with [`exec`] (buttons), [`select_exec`] (selection menus) or
/// [`submit`] (modals).
pub struct Exec<T, E, S = ()> {
    pub(crate) f: std::sync::Arc<
        dyn for<'a> Fn(&'a Ctx<'a, E>, T, Selection<S>) -> BoxFuture<'a, Result<()>> + Send + Sync,
    >,
}

impl<T, E, S> std::fmt::Debug for Exec<T, E, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Exec(..)")
    }
}

impl<T, E, S> Clone for Exec<T, E, S> {
    fn clone(&self) -> Self {
        Self { f: self.f.clone() }
    }
}

/// Wrapper marking a two-argument execution callback (buttons).
pub struct FnExec<T, E, F>(pub F, pub PhantomData<fn() -> (T, E)>);

/// Mark a closure as an interactive execution: `|ctx, data| Box::pin(async move { ... })`.
pub fn exec<T, E, F>(f: F) -> FnExec<T, E, F>
where
    T: Send + 'static,
    E: Send + Sync + 'static,
    F: for<'a> Fn(&'a Ctx<'a, E>, T) -> BoxFuture<'a, Result<()>> + Send + Sync + 'static,
{
    FnExec(f, PhantomData)
}

impl<T: Send + 'static, E: Send + Sync + 'static, F> From<FnExec<T, E, F>> for Exec<T, E, ()>
where
    F: for<'a> Fn(&'a Ctx<'a, E>, T) -> BoxFuture<'a, Result<()>> + Send + Sync + 'static,
{
    fn from(v: FnExec<T, E, F>) -> Self {
        let f = v.0;
        Exec {
            f: std::sync::Arc::new(move |ctx, data, _sel| f(ctx, data)),
        }
    }
}

/// Wrapper marking a three-argument execution callback (selection menus).
pub struct SelectFn<T, E, S, F>(pub F, pub PhantomData<fn() -> (T, E, S)>);

/// Mark a closure as a selection-menu execution:
/// `|ctx, data, selection| Box::pin(async move { ... })`.
pub fn select_exec<T, E, S, F>(f: F) -> SelectFn<T, E, S, F>
where
    T: Send + 'static,
    E: Send + Sync + 'static,
    S: SelectValues,
    F: for<'a> Fn(&'a Ctx<'a, E>, T, Selection<S>) -> BoxFuture<'a, Result<()>>
        + Send
        + Sync
        + 'static,
{
    SelectFn(f, PhantomData)
}

impl<T: Send + 'static, E: Send + Sync + 'static, S: SelectValues, F> From<SelectFn<T, E, S, F>>
    for Exec<T, E, S>
where
    F: for<'a> Fn(&'a Ctx<'a, E>, T, Selection<S>) -> BoxFuture<'a, Result<()>>
        + Send
        + Sync
        + 'static,
{
    fn from(v: SelectFn<T, E, S, F>) -> Self {
        Exec {
            f: std::sync::Arc::new(v.0),
        }
    }
}

/// Wrapper marking a modal submission callback.
pub struct SubmitFn<T, E, F>(pub F, pub PhantomData<fn() -> (T, E)>);

/// Mark a closure as a Modal `on_submit` execution:
/// `|ctx, data, submission| Box::pin(async move { ... })`.
pub fn submit<T, E, F>(f: F) -> SubmitFn<T, E, F>
where
    T: Send + 'static,
    E: Send + Sync + 'static,
    F: for<'a> Fn(&'a Ctx<'a, E>, T, ModalSubmission) -> BoxFuture<'a, Result<()>>
        + Send
        + Sync
        + 'static,
{
    SubmitFn(f, PhantomData)
}

/// A type-erased execution (over the selection type): deserializes the
/// persisted data and invokes the callback. Closures themselves are never
/// stored in the database; the registered definition holds them.
pub(crate) trait ErasedExec<E>: Send + Sync {
    fn invoke_erased<'a>(
        &'a self,
        ctx: &'a Ctx<'a, E>,
        data_json: &'a [u8],
    ) -> BoxFuture<'a, Result<()>>;
}

impl<T, E, S> ErasedExec<E> for Exec<T, E, S>
where
    T: serde::de::DeserializeOwned + Send + 'static,
    E: Send + Sync + 'static,
    S: serde::de::DeserializeOwned + SelectValues + Default,
{
    fn invoke_erased<'a>(
        &'a self,
        ctx: &'a Ctx<'a, E>,
        data_json: &'a [u8],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let data: T =
                serde_json::from_slice(data_json).map_err(|source| Error::Deserialize {
                    def_id: "execution".to_string(),
                    source,
                })?;
            let json = ctx
                .selection_json
                .clone()
                .unwrap_or_else(|| serde_json::json!({}));
            let selection: Selection<S> =
                serde_json::from_value(json).map_err(|source| Error::Deserialize {
                    def_id: S::KIND.to_string(),
                    source,
                })?;
            (self.f)(ctx, data, selection).await
        })
    }
}

/// A Frame: a dynamic region inside a Page that resolves its own component
/// hierarchy at render time and can paginate its contents.
impl<T, E> std::fmt::Debug for FrameSlot<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FrameSlot(..)")
    }
}

pub struct FrameSlot<T, E> {
    pub(crate) f: std::sync::Arc<
        dyn for<'a> Fn(
                &'a crate::context::RenderCtx<'a, T, E>,
                T,
            ) -> BoxFuture<'a, Result<Vec<Element<T, E>>>>
            + Send
            + Sync,
    >,
}

/// Wrapper marking a Frame callback.
pub struct FrameFn<T, E, F>(pub F, pub PhantomData<fn() -> (T, E)>);

/// Mark a closure as a Frame callback:
/// `|ctx, data| Box::pin(async move { Ok(vec![Element::text(..)]) })`.
pub fn frame<T, E, F>(f: F) -> FrameFn<T, E, F>
where
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    F: for<'a> Fn(
            &'a crate::context::RenderCtx<'a, T, E>,
            T,
        ) -> BoxFuture<'a, Result<Vec<Element<T, E>>>>
        + Send
        + Sync
        + 'static,
{
    FrameFn(f, PhantomData)
}

impl<T: Send + Sync + 'static, E: Send + Sync + 'static, F> From<FrameFn<T, E, F>>
    for FrameSlot<T, E>
where
    F: for<'a> Fn(
            &'a crate::context::RenderCtx<'a, T, E>,
            T,
        ) -> BoxFuture<'a, Result<Vec<Element<T, E>>>>
        + Send
        + Sync
        + 'static,
{
    fn from(v: FrameFn<T, E, F>) -> Self {
        FrameSlot {
            f: std::sync::Arc::new(v.0),
        }
    }
}

/// A Modal submission callback slot.
pub struct SubmitSlot<T, E> {
    pub(crate) f: std::sync::Arc<
        dyn for<'a> Fn(&'a Ctx<'a, E>, T, ModalSubmission) -> BoxFuture<'a, Result<()>>
            + Send
            + Sync,
    >,
}

impl<T, E> std::fmt::Debug for SubmitSlot<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SubmitSlot(..)")
    }
}

impl<T, E, F> From<SubmitFn<T, E, F>> for SubmitSlot<T, E>
where
    T: Send + 'static,
    E: Send + Sync + 'static,
    F: for<'a> Fn(&'a Ctx<'a, E>, T, ModalSubmission) -> BoxFuture<'a, Result<()>>
        + Send
        + Sync
        + 'static,
{
    fn from(v: SubmitFn<T, E, F>) -> Self {
        SubmitSlot {
            f: std::sync::Arc::new(v.0),
        }
    }
}

/// Attachment IDs submitted through a File Upload input.
pub type SubmittedFiles = Vec<AttachmentId>;
