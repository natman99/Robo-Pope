# Discord UI Framework

## Overview

The framework provides a declarative API for building Discord messages and interactions using Components V2.

A Page describes a message. It defines the component hierarchy, the values displayed by those components, and the executions associated with interactive components. An App manages registered Pages, database persistence, rendering, and interaction routing.

The framework should make building a Discord interface feel like composing ordinary Rust values. Serenity's component builders, custom IDs, serialization, and interaction routing are implementation details.

The framework has four main concepts:

- `App`: Owns application state, the database, registered Pages, and interaction routing.
    
- `Page<T>`: A blueprint for a Discord message, parameterized by its data.
    
- `Modal<T>`: A blueprint for a form submitted through a Discord interaction.
    
- `Frame<T>`: A dynamic region inside a Page that resolves its own component hierarchy at render time and can paginate its contents.
    

## 1. Page

A Page is a reusable blueprint. It defines the structure of a message, not a particular instance of that message.

Each Page has a globally unique ID and a single generic data type, `T`. This data is available to every dynamic component and interactive execution belonging to the Page.

```
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct Dataless;

let page = Page::<Dataless>::new("TheIdOfThePage")
    .add_text("Hello, world!")
    .register(app)?;
```

Pages are built before they are registered. Registration consumes the builder and produces a `RegisteredPage<T>`.

```
let page: RegisteredPage<Dataless> =
    Page::<Dataless>::new("TheIdOfThePage")
        .add_text("Hello, world!")
        .register(app)?;
```

A regular `Page<T>` can be modified. A `RegisteredPage<T>` cannot. This is a typestate distinction: once registered, the Page's definition is fixed.

Rendering is only available on registered Pages.

```
let message = page.render(app, Dataless).await?;
```

Rendering requires the App because interactive components must be registered with the application's persistence layer before their message can be sent.

### Page data

The Page's generic type is the shared data scope for its components and executions.

```
#[derive(Serialize, Deserialize)]
pub struct UserLevelInfo {
    user_id: String,
}

let page = Page::<UserLevelInfo>::new("UserLevel")
    .add_text(|ctx, data| async move {
        let user = ctx.app.db.users.query(data.user_id).await?;
        Ok(format!("Level: {}", user.level))
    })
    .register(app)?;
```

The data passed to `render` is serialized and persisted alongside the Page's interactive execution references. When an interaction occurs, the framework reconstructs the original data and passes it to the corresponding execution.

The data type must implement `Serialize` and `DeserializeOwned`, and satisfy the concurrency requirements of the asynchronous application.

## 2. Static and dynamic values

A component property should accept either a static value or a callback that resolves to that value.

The public API should not require separate constructors or methods for static and dynamic values.

```
Page::<Dataless>::new("Example")
    .add_text("This is static text.")
    .add_text(|ctx, data| async move {
        Ok("This is dynamically generated text.".to_string())
    });
```

The same principle applies to component properties.

```
Button::new(
    "Show Level",
    "ButtonId",
    ButtonStyle::Secondary,
    |ctx, data| async move {
        Ok(())
    },
);
```

The button's label and style should both accept static values or callbacks. The execution remains an explicit callback.

### Into conversions

The implementation should use a common internal representation for values that can be either static or dynamic.

Conceptually:

```
enum Value<T> {
    Static(T),
    Dynamic(/* asynchronous callback returning T */),
}
```

Component methods should accept values convertible into this representation.

```
.add_text("Hello")
.add_text(dynamic_callback)
```

This should apply consistently to properties that can reasonably be resolved at render time:

- Text content and descriptions.
    
- Button labels and styles.
    
- Select menu placeholders and options.
    
- Thumbnail URLs and descriptions.
    
- Container accent colors.
    
- Media URLs.
    
- Disabled states and other component settings.
    

The default rule is that a property accepts either its ordinary value or an asynchronous callback returning that value, unless there is a specific reason not to.

Dynamic callbacks are evaluated when the Page is rendered. They are not interactive executions and do not need their own database entries.

A dynamic value determines what a component looks like. An execution determines what happens when someone interacts with a component.

The latter remains an explicit callback.

### Fixed slots and compile-time component counts

Some Discord components have a fixed maximum number of children. Rather than accepting an arbitrary array and discovering invalid configurations at runtime, the API should expose a fixed number of slots.

Each slot can contain a static value, a dynamic callback, or nothing.

For example, a Section supports up to three Text Displays:

```
Section::new()
    .set_text(
        "User information",
        None,
        None,
    )
```

A Section with three text blocks:

```
Section::new()
    .set_text(
        "User information",
        dynamic_description,
        "Additional information",
    )
```

The same idea applies to button rows:

```
.add_button_row((
    Some(button1),
    Some(button2),
    None,
    None,
    None,
))
```

A fully populated row:

```
.add_button_row((
    Some(button1),
    Some(button2),
    Some(button3),
    Some(button4),
    Some(button5),
))
```

This makes the maximum number of slots explicit in the API. An omitted slot is represented by `None`, rather than relying on runtime validation to discover an invalid number of children.

The implementation should use a fixed-size representation, such as a tuple or fixed-size array, and convert it into the appropriate Serenity component structure.

The exact conversion types should be tested for Rust inference and ergonomic `Into` behavior before the signatures are finalized. In particular, `None` must not require awkward type annotations.

## 3. Components V2

The framework should expose Discord's Components V2 functionality through a composable API.

The API should model how developers build interfaces, rather than mechanically exposing every Serenity builder method.

### Text Display

A Text Display presents Markdown-formatted text.

```
Page::<Dataless>::new("Example")
    .add_text("# Welcome")
    .add_text("This is another text block.");
```

Each call adds a separate Text Display. Text should not be silently concatenated with an existing component.

Dynamic content uses the same method:

```
.add_text(|ctx, data| async move {
    Ok(format!("Welcome, {}!", data.username))
})
```

### Section

A Section places text alongside an accessory. Discord supports one to three Text Displays and an accessory.

The public API should expose three explicit text slots.

```
Section::new()
    .set_text(
        "User information",
        "Account details",
        None,
    )
    .set_accessory(
        Thumbnail::new("https://example.com/avatar.png")
    )
```

Each text slot can independently be static, dynamic, or absent.

The accessory can be a Thumbnail or a Button.

```
Section::new()
    .set_text(
        "View additional information.",
        None,
        None,
    )
    .set_accessory(
        Button::new(
            "View",
            "ViewButton",
            ButtonStyle::Secondary,
            |ctx, data| async move {
                Ok(())
            },
        )
    )
```

The framework should ensure that the resulting Section satisfies Discord's required structure.

### Container

A Container groups components into a visually distinct block. It can have an accent color and spoiler behavior.

```
Container::new()
    .set_accent_color(0x5865F2)
    .add_text("Account information")
    .add_separator()
    .add_text("Additional details")
```

Containers should support the component types permitted by Discord.

### Separator

A Separator provides visual spacing and can optionally display a divider.

```
Separator::new()
    .set_spacing(SeparatorSpacing::Large)
    .set_divider(true)
```

### Thumbnail

A Thumbnail displays a small image, usually as a Section accessory.

```
Thumbnail::new("https://example.com/avatar.png")
    .set_description("User avatar")
```

The image source and description should support static or dynamic values.

### Media Gallery

A Media Gallery displays a collection of images or videos.

```
MediaGallery::new()
    .add_media("https://example.com/image1.png")
    .add_media("https://example.com/image2.png")
```

Individual media items may have descriptions.

### File

A File component displays an uploaded attachment within a Components V2 message.

```
File::new("document.pdf")
```

The framework should associate the component with the corresponding uploaded attachment. Uploading a file and displaying it are related operations, but they are not the same operation.

## 4. Interactive components

Interactive components create executions. Each execution has an ID unique within its Page.

### Button

```
Button::new(
    "Show Level",
    "ButtonId",
    ButtonStyle::Secondary,
    |ctx, data: UserLevelInfo| async move {
        let level_info = ctx.app.db.user_level.query(data.user_id).await?;

        let render = ctx
            .app
            .pages
            .query("ShowUserLevel")?
            .render(ctx.app, ShowUserLevelData {
                level: level_info.level,
            })
            .await?;

        ctx.edit_message(render).await?;

        Ok(())
    },
)
```

The label and style can be static or dynamic. The execution is always an explicit callback.

### Link Button

A Link Button opens a URL rather than invoking an application execution.

It should be a separate component because it does not produce an interaction.

```
LinkButton::new(
    "Visit Website",
    "https://example.com",
)
```

Its label and URL should support static or dynamic values.

### Button rows

Developers should not have to manually create Action Rows.

```
Page::<Dataless>::new("Example")
    .add_button_row((
        Some(button1),
        Some(button2),
        None,
        None,
        None,
    ))
```

The framework creates the underlying Action Row.

Discord supports up to five buttons in an Action Row. Empty slots are explicitly represented by `None`.

### Selection menus

A Page should expose one method for adding a selection menu.

```
.add_selection_menu(selection_menu)
```

The selection menu itself determines its type.

The framework should support:

- String Select.
    
- User Select.
    
- Role Select.
    
- Mentionable Select.
    
- Channel Select.
    

These can be represented by distinct selection-menu types implementing a shared trait, or by an enum that unites the supported types.

The important point is that the Page API accepts one selection-menu abstraction rather than requiring separate Page methods for every Discord select type.

A String Select might look like this:

```
StringSelect::new(
    "Choose a class",
    "ClassSelect",
    [
        SelectOption::new("Warrior", "warrior"),
        SelectOption::new("Mage", "mage"),
        SelectOption::new("Rogue", "rogue"),
    ],
    |ctx, data, selection| async move {
        // Handle the selected value or values.
        Ok(())
    },
)
```

The option labels, descriptions, and other display properties should support static and dynamic values.

The selected values are interaction data. They are not part of the Page's serialized data.

Entity select menus return Discord entities rather than arbitrary strings. Their callbacks should expose the appropriate selected users, roles, channels, or mentionables.

## 5. Frame

A Frame is a dynamic region inside a Page.

Unlike a dynamic property, which resolves one component value, a Frame resolves an entire collection of components.

This allows a Page to have a fixed structure while still displaying a dynamically generated list of components.

```
Page::<UserListData>::new("UserList")
    .add_text("# Users")
    .add_frame(|ctx, data| async move {
        let users = ctx.app.db.users.query_all().await?;

        Ok(users.into_iter()
            .map(|user| {
                Element::text(format!(
                    "{} - Level {}",
                    user.name,
                    user.level
                ))
            })
            .collect())
    })
    .add_text("End of user list")
    .register(app)?;
```

The Page's static components remain fixed. The Frame's contents are resolved at render time.

### Frame contents

A Frame should be able to return a collection of heterogeneous components.

For example, a Frame may contain Text Displays, Sections, Containers, Separators, and interactive components.

Conceptually, these components should be represented by a common `Element` abstraction.

```
Ok(vec![
    Element::text("User information"),
    Element::separator(),
    Element::section(
        Section::new()
            .set_text("Account details", None, None)
    ),
])
```

The exact representation is an implementation detail. The public API should allow developers to compose different component types without manually converting everything into Serenity builders.

Interactive components inside a Frame must receive stable execution identities. Their execution IDs should be derived from stable developer-defined identifiers or another deterministic identity mechanism, not their temporary position in a generated list.

### Frame pagination

A Frame can contain more components than Discord permits in a single message.

Rather than requiring developers to manually split their data into pages, the framework should automatically paginate the Frame's contents.

```
Page::<UserListData>::new("UserList")
    .add_text("# Users")
    .add_frame(|ctx, data| async move {
        let users = ctx.app.db.users.query_all().await?;

        Ok(users.into_iter()
            .map(|user| {
                Element::text(format!("{} - Level {}", user.name, user.level))
            })
            .collect())
    })
    .paginate()
    .register(app)?;
```

The Frame should calculate how many elements fit into each rendered message.

The pagination algorithm must account for:

- The maximum total component count.
    
- The combined text character limit.
    
- Nested component counts.
    
- Container child limits.
    
- Section text limits.
    
- Media Gallery item limits.
    
- Any other Discord component restrictions.
    

The framework should maximize the amount of Frame content displayed in each page without exceeding any Discord limits.

### Pagination boundaries

Pagination should split only between complete components.

It should not silently truncate a component or split an interactive component into invalid pieces.

If a single component exceeds a Discord limit, rendering should return a descriptive error unless that component has an explicitly supported splitting strategy.

For example, an oversized Text Display could potentially be split into multiple Text Displays, but that should be a deliberate behavior rather than an accidental consequence of pagination.

### Static content around a Frame

The Frame does not own the entire Page.

Static elements before and after the Frame are part of the Page's fixed structure.

```
Page::<UserListData>::new("UserList")
    .add_text("# User Directory")
    .add_separator()
    .add_frame(dynamic_users)
    .add_separator()
    .add_text("Use the controls below to browse.")
    .paginate()
    .register(app)?;
```

When the Frame has multiple pages, the static surrounding components should remain present on every rendered page.

The pagination algorithm must reserve the component and text budget needed by the static Page elements before determining how much Frame content fits.

### Pagination controls

Pagination should be handled by the framework.

The framework should provide navigation controls for moving between generated pages. These controls should not require the developer to define ordinary button callbacks.

The current pagination position is part of the rendered Page state. Moving to another page should update that state and render the corresponding Frame contents.

The framework should prevent navigation beyond the first or last page.

### Pagination state and persistence

Pagination requires state beyond the original Page data.

The framework should distinguish:

- The Page's original serialized data.
    
- The Frame's resolved content or content source.
    
- The current pagination position.
    

The current page index must be persisted or otherwise recoverable when the user interacts with navigation controls.

The framework should also avoid relying on the current array position of an interactive component as its identity. Moving an element to another page must not accidentally associate it with another execution.

For large datasets, the Frame should eventually support a paginated data source so the framework does not need to load an enormous collection into memory before displaying the first page.

The initial implementation can support a collection returned by the Frame callback, with data-source pagination added later.

## 6. Modals

A Modal describes a form that Discord displays after an interaction.

A Modal is not a message component. It is opened in response to an interaction and produces a separate interaction when submitted.

A Modal should be its own blueprint type.

```
#[derive(Serialize, Deserialize)]
pub struct EditUserData {
    user_id: String,
}

let modal = Modal::<EditUserData>::new("EditUser")
    .set_title("Edit User")
    .add_text_input(
        "username",
        "Username",
        TextInputStyle::Short,
    )
    .add_text_input(
        "description",
        "Description",
        TextInputStyle::Paragraph,
    )
    .on_submit(|ctx, data, submission| async move {
        let username = submission.text("username")?;
        let description = submission.text("description")?;

        ctx.app.db.users.update(
            data.user_id,
            username,
            description,
        ).await?;

        ctx.reply("User updated successfully.").await?;

        Ok(())
    })
    .register(app)?;
```

The Modal's generic data works like a Page's data. It is serialized when the Modal is opened and reconstructed when the form is submitted.

The submitted form values are separate. They come from Discord and are provided to the submission callback through a `ModalSubmission` object.

### Opening a Modal

A Modal is opened by an existing interaction, usually a button execution.

```
Button::new(
    "Edit User",
    "EditUserButton",
    ButtonStyle::Primary,
    |ctx, data: UserLevelInfo| async move {
        let modal = ctx.app.modals
            .query("EditUser")?
            .render(ctx.app, EditUserData {
                user_id: data.user_id,
            })
            .await?;

        ctx.open_modal(modal).await?;

        Ok(())
    },
)
```

Opening a Modal is an interaction response. It is not an ordinary message edit or follow-up.

### Modal components

The Modal API should support the input components Discord permits:

- Text Input.
    
- String Select.
    
- User Select.
    
- Role Select.
    
- Mentionable Select.
    
- Channel Select.
    
- File Upload.
    
- Radio Group.
    
- Checkbox Group.
    
- Checkbox.
    

The API should use simple input definitions and handle Discord's required Label structures internally.

```
Modal::<Dataless>::new("CreateTicket")
    .set_title("Create a Ticket")
    .add_text_input(
        "subject",
        "Subject",
        TextInputStyle::Short,
    )
    .add_text_input(
        "details",
        "Describe your issue",
        TextInputStyle::Paragraph,
    )
    .add_select(
        "category",
        "Category",
        categories,
    )
    .add_checkbox(
        "urgent",
        "This issue is urgent",
    )
    .on_submit(|ctx, data, submission| async move {
        Ok(())
    })
```

Modal inputs should have stable developer-defined IDs. Those IDs are used to retrieve their submitted values.

### Typed submission data

The long-term goal should be to support deserializing Modal submissions into a developer-defined Rust struct.

```
#[derive(Deserialize)]
struct EditUserForm {
    username: String,
    description: String,
}

.on_submit(|ctx, data, form: EditUserForm| async move {
    ctx.app.db.users.update(
        data.user_id,
        form.username,
        form.description,
    ).await?;

    Ok(())
})
```

This avoids requiring developers to manually retrieve strings by component ID and then parse them.

The framework should map component IDs to struct fields and convert submitted values into the appropriate Rust types. Missing or invalid values should return errors.

The exact mechanism for this should be designed around Serde rather than inventing a second serialization system.

## 7. Interaction Context

Every execution receives an interaction context.

The context represents the current Discord interaction and provides access to both the App and interaction-specific operations.

```
|ctx, data| async move {
    let user = ctx.app.db.users.query(data.user_id).await?;

    ctx.reply("Hello!").await?;

    Ok(())
}
```

The context should expose operations relevant to the interaction type:

- Reply to an interaction.
    
- Defer an interaction response.
    
- Edit the message containing the component.
    
- Edit the original interaction response.
    
- Send a follow-up message.
    
- Delete an interaction response.
    
- Open a Modal.
    

The context also exposes interaction information, such as the user who interacted, the guild and channel, selected values, and submitted Modal data.

The context is temporary. It belongs to the current interaction and is never stored inside a Page or persisted in the database.

## 8. App and registration

The App owns the registered Pages and Modals, the database, and the interaction router.

```
struct App {
    db: Database,
    pages: PageRegistry,
    modals: ModalRegistry,
}
```

The registries must be able to hold Pages and Modals with different generic data types.

The public API should preserve the concrete generic types when defining and using them. Type erasure is an internal registry implementation detail.

A registered Page is immutable. The App owns its registered definition and uses it to resolve executions.

Rendering a Page requires the App:

```
let message = app.pages
    .query("UserLevel")?
    .render(&app, user_data)
    .await?;
```

The App should not be stored inside the Page. The App is supplied when registration or rendering requires application services.

## 9. Persistence and interaction routing

Discord returns a component's `custom_id` when a user interacts with it. The framework uses that value to locate the execution.

The database stores the mapping between the custom ID and the execution definition.

Each record contains:

1. A deterministic hash, used as the Discord custom ID.
    
2. The globally unique Page or Modal ID.
    
3. The execution ID, unique within that definition.
    
4. The serialized data associated with the rendered instance.
    

The hash is derived from the Page or Modal ID, execution ID, and serialized data. The same definition and data should produce the same hash.

This allows repeated renders of the same definition and data to reuse the same database record.

The hash should be deterministic, using a stable serialization format and a deterministic hashing algorithm such as BLAKE3.

Closures themselves are never serialized or stored in the database. The registered definition contains the actual callback. The database only stores the information needed to locate it.

### Routing

When an interaction arrives:

1. The App receives the Discord interaction.
    
2. The App reads the custom ID.
    
3. The database resolves the hash to its stored record.
    
4. The App retrieves the registered Page or Modal.
    
5. The Page or Modal resolves the execution ID.
    
6. The serialized data is deserialized into its original generic type.
    
7. The callback is invoked with the interaction context and reconstructed data.
    

The App does not need to know the generic data type. The registered Page or Modal handles deserialization and execution internally.

A valid custom ID does not grant permission to perform an operation. Authorization must be checked inside the execution.

### Frame executions

Interactive elements generated inside a Frame need additional identity information.

Their identities must remain stable across pagination and rerendering. The identity should not depend solely on their current visual position.

A Frame should provide a stable identity for each generated interactive element, derived from a developer-defined key or another deterministic identifier.

This ensures that moving an element between pagination boundaries does not change which callback it invokes.

## 10. Errors

The framework should use ordinary Rust `Result` values for operations that can fail.

Examples include:

- Registering duplicate Page or Modal IDs.
    
- Registering duplicate execution IDs within a Page.
    
- Rendering a component with invalid configuration.
    
- Resolving an unknown Page or Modal.
    
- Resolving an unknown execution.
    
- Deserializing persisted data.
    
- Retrieving a missing or invalid Modal submission value.
    
- Resolving an invalid Frame.
    
- Encountering a component that cannot fit within Discord's limits.
    
- Sending an invalid Discord component structure.
    

Errors should preserve useful context, especially the Page or Modal ID and execution ID involved.

Fixed-size component slots should prevent invalid child counts from being expressible through the ordinary public API wherever possible. Runtime validation is still necessary for constraints that depend on resolved dynamic values.

## 11. Design principles

**One public interface for static and dynamic values.** A property accepts either its ordinary value or a callback that resolves to that value.

**Executions are explicit.** A button click, selection, or Modal submission always has an explicit callback.

**Page data is shared.** Components and executions belonging to a Page use the same generic data type.

**Structure is declarative.** The Page defines its fixed structure. Frames provide dynamic regions.

**Frames own dynamic collections.** A Frame resolves components at render time and handles pagination without forcing the developer to implement navigation.

**Component limits should be represented in the API.** Fixed-size slots should make Discord's structural constraints explicit rather than relying on avoidable runtime errors.

**Discord implementation details stay hidden.** The framework handles component nesting, custom IDs, serialization, and interaction routing.

**Registered definitions are immutable.** Build first, register once, then render.

**Persistence survives restarts.** The database stores enough information to route interactions back to the correct Rust callbacks.

**Errors are explicit.** Invalid persisted references, missing values, and failed executions return errors instead of panicking.

## 12. Discord constraints

The implementation must respect Discord's actual component limits.

For Components V2 messages:

- A message can contain up to 40 total components, including nested components.
    
- Text Display content shares a 4,000-character message-wide budget.
    
- A Section supports one to three Text Displays and one accessory.
    
- A button Action Row supports one to five buttons.
    
- A select menu occupies its own Action Row.
    
- A String Select supports up to 25 options.
    
- A Media Gallery supports up to 10 media items.
    
- A Container supports up to 10 direct child components.
    
- A component's custom ID is limited to 100 characters.
    

Components V2 messages require the `IS_COMPONENTS_V2` flag. Traditional message content and embeds cannot be used in the normal way with Components V2.

For Modals:

- A Modal title is limited to 45 characters.
    
- A Modal supports up to five top-level components.
    
- Input labels are limited to 45 characters.
    
- Label descriptions are limited to 100 characters.
    
- Text Inputs support up to 4,000 characters.
    
- Select menus support up to 25 options.
    

These constraints should be validated by the framework, including after dynamic values and Frame contents have been resolved.

The framework should not assume that a Page is valid merely because its static definition is valid. Dynamic content can exceed the limits at render time.

## 13. Implementation priorities

The framework should be implemented in stages.

1. Page builder and registration.
    
2. Static and dynamic property resolution.
    
3. Component composition and fixed-size slots.
    
4. Button executions and persistence.
    
5. Selection menus and interaction routing.
    
6. Modal registration and submission handling.
    
7. Frame component generation.
    
8. Frame pagination and automatic navigation.
    
9. Typed Modal submission deserialization.
    
10. More advanced Frame data sources and pagination optimizations.
    

This order establishes the core execution and rendering model before adding the more complex dynamic collection and pagination behavior.

The end result should make building a Discord interface mostly ordinary composition:

```
let page = Page::<UserListData>::new("UserList")
    .add_container(
        Container::new()
            .add_text("# User Directory")
            .add_separator()
            .add_frame(|ctx, data| async move {
                let users = ctx.app.db.users.query_all().await?;

                Ok(users.into_iter()
                    .map(|user| {
                        Element::text(format!(
                            "{} - Level {}",
                            user.name,
                            user.level
                        ))
                    })
                    .collect())
            })
            .add_button_row((
                Some(previous_button),
                Some(next_button),
                None,
                None,
                None,
            ))
    )
    .register(app)?;
```

The framework handles turning this definition into valid Discord messages, splitting dynamic content into pages, persisting interactive executions, and routing future interactions back to the correct Rust callbacks.