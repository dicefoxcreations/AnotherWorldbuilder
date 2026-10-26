//! The commands a member may have done where a world is held — by this
//! app's own server for its members, or by `aw-server`: each the same
//! function the app calls for itself, run in the member's name
//! (`store::acting_for`), so the same rules hold wherever it runs.
//!
//! What names a file on the asking device — a picture to upload, a player's
//! card — travels as its content instead (`image_put`, `member_admit`).

use super::*;
use serde::de::DeserializeOwned;

fn arg<T: DeserializeOwned>(args: &serde_json::Value, key: &str) -> Answer<T> {
    Ok(serde_json::from_value(args.get(key).cloned().unwrap_or(serde_json::Value::Null))?)
}

fn json<T: Serialize>(value: T) -> Answer<serde_json::Value> {
    Ok(serde_json::to_value(value)?)
}

/// Does `command` in `world` for the one this runs for, with `args` as the
/// frontend sends them. A command not named here is not done from afar.
pub(crate) fn call(store: &Store, world: &str, command: &str, args: &serde_json::Value) -> Answer<serde_json::Value> {
    let w = || world.to_string();
    match command {
        // The world as a whole.
        "world_connect" => json(world_now(store, world)?),
        "world_icon_set" => json(world_icon_set_in(store, w(), arg(args, "icon")?)?),
        "calendar_set" => json(calendar_set_in(store, w(), arg(args, "calendar")?)?),
        "world_moderation_set" => json(world_moderation_set_in(store, w(), arg(args, "on")?)?),
        "world_versions_set" => json(world_versions_set_in(store, w(), arg(args, "max")?)?),
        "world_listed_set" => json(world_listed_set_in(store, w(), arg(args, "listed")?)?),
        "world_dice_set" => json(world_dice_set_in(store, w(), arg(args, "sides")?)?),
        "today_in_world" => json(today_in_world_in(store, w(), arg(args, "day")?)?),
        "navigation_load" => json(navigation_load_in(store, w())?),
        "navigation_set" => json(navigation_set_in(store, w(), arg(args, "entries")?)?),
        "world_revision" => json(store.db(world)?.revision()),
        // Members and roles.
        "member_admit" => {
            admit(store, world, arg(args, "card")?, arg(args, "attributes")?)?;
            json(world_now(store, world)?)
        }
        "member_colour_set" => json(member_colour_set_in(store, w(), arg(args, "colour")?)?),
        "member_replace" => json(member_replace_in(store, w(), arg(args, "old")?, arg(args, "new")?)?),
        "member_remove" => json(member_remove_in(store, w(), arg(args, "id")?)?),
        "member_trust_set" => json(member_trust_set_in(store, w(), arg(args, "member")?, arg(args, "trusted")?)?),
        "member_attributes_set" => {
            member_attributes_set_in(store, world, &arg::<String>(args, "member")?, arg(args, "attributes")?)?;
            json(world_now(store, world)?)
        }
        "member_invite" => json(network::member_invite_in(store, w(), arg(args, "id")?)?),
        "world_supporters_set" => json(world_supporters_set_in(store, w(), arg(args, "ids")?)?),
        "world_take_over" => json(world_take_over_in(store, w())?),
        "gm_handover" => json(gm_handover_in(store, w(), arg(args, "member")?)?),
        "attribute_create" => json(attribute_create_in(store, w(), arg(args, "attribute")?)?),
        "attribute_revoke" => json(attribute_revoke_in(store, w(), arg(args, "attribute")?)?),
        "join_requests" => json(network::join_requests_in(store, w())?),
        "join_answer" => json(network::join_answer_in(store, w(), arg(args, "id")?, arg(args, "accept")?)?),
        // Types.
        "templates_load" => json(templates_load_in(store, w())?),
        "template_save" => json(save_template(store, world, arg(args, "template")?)?),
        "template_delete" => json(delete_template(store, world, &arg::<String>(args, "slug")?)?),
        "template_read" => json(template_read_in(store, w(), arg(args, "text")?, arg(args, "base")?)?),
        "template_write" => json(template_write_in(store, w(), arg(args, "template")?)?),
        // Documents.
        "view" => json(view_in(store, w(), arg(args, "previewAs")?)?),
        "article_create" => json(article_create_in(store, w(), arg(args, "articleType")?, arg(args, "slug")?, arg(args, "name")?, arg(args, "visibility")?, arg(args, "layers")?)?),
        "article_propose" => json(propose_document(store, world, arg(args, "articleType")?, arg(args, "slug")?, arg(args, "name")?, arg(args, "layers")?)?),
        "article_delete" => json(article_delete_in(store, w(), arg(args, "article")?, arg(args, "confirmation")?)?),
        "texts_written" => json(texts_written_in(store, w(), arg(args, "articleType")?, arg(args, "texts")?)?),
        "visibility_set" => json(visibility_set_in(store, world, &arg::<String>(args, "article")?, arg::<Option<String>>(args, "layer")?.as_deref(), &arg::<String>(args, "policy")?)?),
        "type_access_apply" => json(type_access_apply_in(store, world, &arg::<String>(args, "articleType")?, &arg::<Vec<String>>(args, "articles")?, arg(args, "view")?, arg(args, "edit")?)?),
        "document_open" => json(document::document_open_in(store, w(), arg(args, "article")?)?),
        "document_read" => json(document::document_read_in(store, w(), arg(args, "article")?, arg(args, "text")?, arg(args, "base")?)?),
        "document_save" => json(document::document_save_in(store, w(), arg(args, "article")?, arg(args, "text")?, arg(args, "base")?, arg(args, "session")?)?),
        "session_end" => json(document::session_end_in(store, w(), arg(args, "session")?)?),
        "image_field_set" => json(document::image_field_set_in(store, w(), arg(args, "article")?, arg(args, "field")?, arg(args, "image")?)?),
        "references_set" => json(document::references_set_in(store, w(), arg(args, "article")?, arg(args, "field")?, arg(args, "ids")?)?),
        "relations_set" => json(document::relations_set_in(store, w(), arg(args, "changes")?)?),
        // Versions.
        "article_versions" => json(document::history::article_versions_in(store, w(), arg(args, "article")?)?),
        "version_compare" => json(document::history::version_compare_in(store, w(), arg(args, "article")?, arg(args, "session")?)?),
        "version_document" => json(document::history::version_document_in(store, w(), arg(args, "article")?, arg(args, "session")?)?),
        "version_marks" => json(document::history::version_marks_in(store, w(), arg(args, "article")?, arg(args, "session")?)?),
        "version_verdict" => json(document::history::verdict_in(store, world, &arg::<String>(args, "article")?, &arg::<String>(args, "session")?, arg(args, "accepted")?)?),
        "version_delete" => json(document::history::version_delete_in(store, world, &arg::<String>(args, "article")?, &arg::<String>(args, "session")?)?),
        "version_revert" => json(document::history::version_revert_in(store, world, &arg::<String>(args, "article")?, &arg::<String>(args, "session")?, &arg::<String>(args, "into")?)?),
        "waiting_versions" => json(document::waiting_in(store, world)?),
        // Pictures.
        "image_put" => json(media::image_put_in(store, w(), arg(args, "name")?, arg(args, "folder")?, arg(args, "data")?)?),
        "image_rename" => json(media::image_rename_in(store, w(), arg(args, "image")?, arg(args, "name")?)?),
        "image_defaults_set" => json(media::image_defaults_set_in(store, w(), arg(args, "image")?, arg(args, "defaults")?)?),
        "image_move" => json(media::image_move_in(store, w(), arg(args, "image")?, arg(args, "folder")?)?),
        "image_delete" => json(store.delete_images(world, &arg::<Vec<String>>(args, "images")?)?),
        "gallery" => json(media::gallery_in(store, w())?),
        "gallery_folders" => json(store.gallery_folders(world)?),
        "gallery_folders_set" => json(store.set_gallery_folders(world, arg(args, "folders")?)?),
        "image_data" => json(media::image_data_in(store, w(), arg(args, "image")?, arg(args, "preview")?)?),
        // Who is where, and the dice.
        "presence" => json(live::presence_in(store, w(), arg(args, "article")?, arg(args, "caret")?)?),
        "world_presence" => json(live::world_presence_in(store, w(), arg(args, "article")?)?),
        "dice_roll" => json(live::dice_roll_in(store, w(), arg(args, "roll")?)?),
        "dice_rolls" => json(live::dice_rolls_in(store, w())?),
        "dice_clear" => json(live::dice_clear_in(store, world)?),
        _ => Err(AppError(format!("“{command}” is not done for a world on a server."))),
    }
}
