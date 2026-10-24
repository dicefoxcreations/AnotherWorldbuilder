//! Native command dispatch for a server-backed client session.
use super::*;
use serde::de::DeserializeOwned;

fn argument<T: DeserializeOwned>(args: &serde_json::Value, key: &str) -> Answer<T> {
    Ok(serde_json::from_value(args.get(key).cloned().unwrap_or(serde_json::Value::Null))?)
}

pub(super) async fn run(app: tauri::AppHandle, store: Store, world: &str, command: &str, args: &serde_json::Value) -> Answer<serde_json::Value> {
    if command == "world_readable_set" && !store.copies(world)? && argument::<bool>(args, "on")? {
        return Err(AppError("Local copies are disabled for this online world.".into()));
    }
    let world = world.to_string();
    match command {
        "world_copy" => ASYNC_STORE.scope(store, async { Ok(serde_json::to_value(crate::world_copy(app.clone(), world.to_string(), argument(args, "destination")?).await?)?) }).await,
        "world_check" => ASYNC_STORE.scope(store, async { Ok(serde_json::to_value(crate::network::world_check(app.clone(), world.to_string()).await?)?) }).await,
        "package_export" => ASYNC_STORE.scope(store, async { Ok(serde_json::to_value(crate::transfer::package_export(app.clone(), world.to_string(), argument(args, "path")?, argument(args, "choice")?).await?)?) }).await,
        "package_read" => ASYNC_STORE.scope(store, async { Ok(serde_json::to_value(crate::transfer::package_read(app.clone(), world.to_string(), argument(args, "path")?).await?)?) }).await,
        "package_import" => ASYNC_STORE.scope(store, async { Ok(serde_json::to_value(crate::transfer::package_import(app.clone(), world.to_string(), argument(args, "path")?).await?)?) }).await,
        _ => {
            let command = command.to_string();
            let args = args.clone();
            tauri::async_runtime::spawn_blocking(move || with_store(store, || {
                let world = world.as_str();
                let args = &args;
                let value = match command.as_str() {
                    "member_add" => serde_json::to_value(crate::member_add(app.clone(), world.to_string(), argument(args, "path")?, argument(args, "attributes")?)?)?,
                    "member_colour_set" => serde_json::to_value(crate::member_colour_set(app.clone(), world.to_string(), argument(args, "colour")?)?)?,
                    "member_replace" => serde_json::to_value(crate::member_replace(app.clone(), world.to_string(), argument(args, "old")?, argument(args, "new")?)?)?,
                    "world_share" => serde_json::to_value(crate::world_share(app.clone(), world.to_string(), argument(args, "member")?, argument(args, "path")?)?)?,
                    "member_remove" => serde_json::to_value(crate::member_remove(app.clone(), world.to_string(), argument(args, "id")?)?)?,
                    "world_moderation_set" => serde_json::to_value(crate::world_moderation_set(app.clone(), world.to_string(), argument(args, "on")?)?)?,
                    "world_versions_set" => serde_json::to_value(crate::world_versions_set(app.clone(), world.to_string(), argument(args, "max")?)?)?,
                    "world_tidy" => serde_json::to_value(crate::world_tidy(app.clone(), world.to_string())?)?,
                    "world_switch_progress" => serde_json::to_value(crate::world_switch_progress(app.clone(), world.to_string())?)?,
                    "world_readable_set" => serde_json::to_value(crate::world_readable_set(app.clone(), world.to_string(), argument(args, "on")?)?)?,
                    "world_encryption_set" => serde_json::to_value(crate::world_encryption_set(app.clone(), world.to_string(), argument(args, "on")?)?)?,
                    "world_dice_set" => serde_json::to_value(crate::world_dice_set(app.clone(), world.to_string(), argument(args, "sides")?)?)?,
                    "navigation_load" => serde_json::to_value(crate::navigation_load(app.clone(), world.to_string())?)?,
                    "navigation_set" => serde_json::to_value(crate::navigation_set(app.clone(), world.to_string(), argument(args, "entries")?)?)?,
                    "member_trust_set" => serde_json::to_value(crate::member_trust_set(app.clone(), world.to_string(), argument(args, "member")?, argument(args, "trusted")?)?)?,
                    "gm_handover" => serde_json::to_value(crate::gm_handover(app.clone(), world.to_string(), argument(args, "member")?)?)?,
                    "gm_handover_file" => serde_json::to_value(crate::gm_handover_file(app.clone(), world.to_string(), argument(args, "member")?, argument(args, "path")?)?)?,
                    "world_icon_set" => serde_json::to_value(crate::world_icon_set(app.clone(), world.to_string(), argument(args, "icon")?)?)?,
                    "calendar_set" => serde_json::to_value(crate::calendar_set(app.clone(), world.to_string(), argument(args, "calendar")?)?)?,
                    "world_token_check" => serde_json::to_value(crate::world_token_check(app.clone(), world.to_string())?)?,
                    "attribute_create" => serde_json::to_value(crate::attribute_create(app.clone(), world.to_string(), argument(args, "attribute")?)?)?,
                    "member_attributes_set" => serde_json::to_value(crate::member_attributes_set(app.clone(), world.to_string(), argument(args, "member")?, argument(args, "attributes")?)?)?,
                    "attribute_revoke" => serde_json::to_value(crate::attribute_revoke(app.clone(), world.to_string(), argument(args, "attribute")?)?)?,
                    "templates_load" => serde_json::to_value(crate::templates_load(app.clone(), world.to_string())?)?,
                    "template_save" => serde_json::to_value(crate::template_save(app.clone(), world.to_string(), argument(args, "template")?)?)?,
                    "template_delete" => serde_json::to_value(crate::template_delete(app.clone(), world.to_string(), argument(args, "slug")?)?)?,
                    "article_create" => serde_json::to_value(crate::article_create(app.clone(), world.to_string(), argument(args, "articleType")?, argument(args, "slug")?, argument(args, "name")?, argument(args, "visibility")?, argument(args, "layers")?)?)?,
                    "article_propose" => serde_json::to_value(crate::article_propose(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "articleType")?, argument(args, "slug")?, argument(args, "name")?, argument(args, "layers")?)?)?,
                    "texts_written" => serde_json::to_value(crate::texts_written(app.clone(), world.to_string(), argument(args, "articleType")?, argument(args, "texts")?)?)?,
                    "visibility_set" => serde_json::to_value(crate::visibility_set(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "layer")?, argument(args, "policy")?)?)?,
                    "article_delete" => serde_json::to_value(crate::article_delete(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "confirmation")?)?)?,
                    "template_read" => serde_json::to_value(crate::template_read(app.clone(), world.to_string(), argument(args, "text")?, argument(args, "base")?)?)?,
                    "template_write" => serde_json::to_value(crate::template_write(app.clone(), world.to_string(), argument(args, "template")?)?)?,
                    "view" => serde_json::to_value(crate::view(app.clone(), world.to_string(), argument(args, "previewAs")?)?)?,
                    "today_in_world" => serde_json::to_value(crate::today_in_world(app.clone(), world.to_string(), argument(args, "day")?)?)?,
                    "document_open" => serde_json::to_value(crate::document::document_open(app.clone(), world.to_string(), argument(args, "article")?)?)?,
                    "document_read" => serde_json::to_value(crate::document::document_read(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "text")?, argument(args, "base")?)?)?,
                    "document_save" => serde_json::to_value(crate::document::document_save(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "text")?, argument(args, "base")?, argument(args, "session")?)?)?,
                    "session_end" => serde_json::to_value(crate::document::session_end(app.clone(), world.to_string(), argument(args, "session")?)?)?,
                    "image_field_set" => serde_json::to_value(crate::document::image_field_set(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "field")?, argument(args, "image")?)?)?,
                    "references_set" => serde_json::to_value(crate::document::references_set(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "field")?, argument(args, "ids")?)?)?,
                    "relations_set" => serde_json::to_value(crate::document::relations_set(app.clone(), world.to_string(), argument(args, "changes")?)?)?,
                    "article_versions" => serde_json::to_value(crate::document::article_versions(app.clone(), world.to_string(), argument(args, "article")?)?)?,
                    "version_compare" => serde_json::to_value(crate::document::version_compare(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "session")?)?)?,
                    "version_document" => serde_json::to_value(crate::document::version_document(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "session")?)?)?,
                    "version_carry" => serde_json::to_value(crate::document::version_carry(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "session")?)?)?,
                    "version_verdict" => serde_json::to_value(crate::document::version_verdict(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "session")?, argument(args, "accepted")?)?)?,
                    "waiting_versions" => serde_json::to_value(crate::document::waiting_versions(app.clone(), world.to_string())?)?,
                    "version_marks" => serde_json::to_value(crate::document::version_marks(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "session")?)?)?,
                    "version_delete" => serde_json::to_value(crate::document::version_delete(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "session")?)?)?,
                    "version_revert" => serde_json::to_value(crate::document::version_revert(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "session")?, argument(args, "into")?)?)?,
                    "image_upload" => serde_json::to_value(crate::media::image_upload(app.clone(), world.to_string(), argument(args, "path")?, argument(args, "name")?, argument(args, "folder")?)?)?,
                    "image_rename" => serde_json::to_value(crate::media::image_rename(app.clone(), world.to_string(), argument(args, "image")?, argument(args, "name")?)?)?,
                    "image_defaults_set" => serde_json::to_value(crate::media::image_defaults_set(app.clone(), world.to_string(), argument(args, "image")?, argument(args, "defaults")?)?)?,
                    "image_move" => serde_json::to_value(crate::media::image_move(app.clone(), world.to_string(), argument(args, "image")?, argument(args, "folder")?)?)?,
                    "gallery_folders" => serde_json::to_value(crate::media::gallery_folders(app.clone(), world.to_string())?)?,
                    "gallery_folders_set" => serde_json::to_value(crate::media::gallery_folders_set(app.clone(), world.to_string(), argument(args, "folders")?)?)?,
                    "image_delete" => serde_json::to_value(crate::media::image_delete(app.clone(), world.to_string(), argument(args, "images")?)?)?,
                    "gallery" => serde_json::to_value(crate::media::gallery(app.clone(), world.to_string())?)?,
                    "image_data" => serde_json::to_value(crate::media::image_data(app.clone(), world.to_string(), argument(args, "image")?, argument(args, "preview")?)?)?,
                    "dice_roll" => serde_json::to_value(crate::network::dice_roll(app.clone(), world.to_string(), argument(args, "roll")?)?)?,
                    "dice_rolls" => serde_json::to_value(crate::network::dice_rolls(world.to_string())?)?,
                    "network_copies_set" => serde_json::to_value(crate::network::network_copies_set(app.clone(), world.to_string(), argument(args, "copies")?)?)?,
                    "network_share" => serde_json::to_value(crate::network::network_share(app.clone(), world.to_string(), argument(args, "shared")?)?)?,
                    "presence" => serde_json::to_value(crate::network::presence(app.clone(), world.to_string(), argument(args, "article")?, argument(args, "caret")?)?)?,
                    "world_presence" => serde_json::to_value(crate::network::world_presence(app.clone(), world.to_string(), argument(args, "article")?)?)?,
                    "join_requests" => serde_json::to_value(crate::network::join_requests(app.clone(), world.to_string())?)?,
                    "join_answer" => serde_json::to_value(crate::network::join_answer(app.clone(), world.to_string(), argument(args, "id")?, argument(args, "accept")?)?)?,
                    "member_invite" => serde_json::to_value(crate::network::member_invite(app.clone(), world.to_string(), argument(args, "id")?)?)?,
                    "backup_view" => serde_json::to_value(crate::backup::backup_view(app.clone(), world.to_string())?)?,
                    "backup_plan_set" => serde_json::to_value(crate::backup::backup_plan_set(app.clone(), world.to_string(), argument(args, "plan")?)?)?,
                    "backup_now" => serde_json::to_value(crate::backup::backup_now(app.clone(), world.to_string())?)?,
                    "backup_open" => serde_json::to_value(crate::backup::backup_open(app.clone(), world.to_string())?)?,
                    _ => return Err(AppError("This command is not available in an online world.".into())),
                };
                Ok(value)
            })).await.map_err(AppError::from)?
        }
    }
}
