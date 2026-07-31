mod common;

use cognate_engine::EngineError;
use cognate_engine::storage::{AttachmentManager, NotebookManager};
use common::{NotebookTestHarness, TempTestDir, now_nanos};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn test_async_attachments() {
    let temp = TempTestDir::new("async_attachments");
    let manager = NotebookManager::new(temp.path());
    let mut notes = Vec::new();
    manager.create_note("work/todo", &mut notes).await.unwrap();

    // 1. Write dummy image to the note folder
    // PNG payload base64: iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=
    let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";
    let rel_path = AttachmentManager::save_image_from_base64(temp.path(), "work/todo", png_base64)
        .await
        .unwrap();

    // Check formatting
    assert!(rel_path.starts_with("images/img_"));
    assert!(rel_path.ends_with(".png"));

    // 2. Verify file is physically present in target dir
    let full_path = temp.path().join("work/todo").join(&rel_path);
    assert!(full_path.exists());

    // 3. Read image bytes and verify signature
    let bytes =
        AttachmentManager::read_image_bytes(temp.path(), &format!("work/todo/{}", rel_path))
            .await
            .unwrap();
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));

    // 4. Delete attachment and verify removal
    AttachmentManager::delete_attachment(temp.path(), &format!("work/todo/{}", rel_path))
        .await
        .unwrap();
    assert!(!full_path.exists());
}

#[tokio::test]
async fn conditional_attachment_delete_rejects_stale_revisions() {
    let temp = TempTestDir::new("conditional_attachment_delete");
    let manager = NotebookManager::new(temp.path());
    let mut notes = Vec::new();
    manager.create_note("note", &mut notes).await.unwrap();

    let bytes = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let rel_path = AttachmentManager::save_image_bytes(temp.path(), "note", &bytes)
        .await
        .unwrap();
    let current = AttachmentManager::read_attachment_bytes(temp.path(), "note", &rel_path)
        .await
        .unwrap();
    let revision = cognate_engine::storage::attachment_revision(&current);

    AttachmentManager::replace_attachment_bytes(
        temp.path(),
        "note",
        &rel_path,
        &revision,
        &[0x89, b'P', b'N', b'G', 0x01],
    )
    .await
    .unwrap();

    let stale =
        AttachmentManager::delete_attachment_if_match(temp.path(), "note", &rel_path, &revision)
            .await;
    assert!(matches!(stale, Err(EngineError::Conflict { .. })));
    assert!(
        AttachmentManager::read_attachment_bytes(temp.path(), "note", &rel_path)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn test_note_creation_path_traversal_attempts() {
    let harness = NotebookTestHarness::new("path_traversal");
    let manager = harness.manager();
    let mut notes = Vec::new();

    let invalid_paths = vec![
        "../outside",
        "dir/../../outside",
        "a/b/../../../c",
        "/absolute/path",
        "some/dir/..",
        "..",
        ".",
    ];

    for path in invalid_paths {
        let res = manager.create_note(path, &mut notes).await;
        assert!(
            res.is_err(),
            "Expected path '{}' to be rejected with error",
            path
        );
    }
}

#[tokio::test]
async fn test_note_creation_empty_and_whitespace_paths() {
    let harness = NotebookTestHarness::new("empty_whitespace");
    let manager = harness.manager();
    let mut notes = Vec::new();

    let invalid = vec!["", "   ", "\n", "\t"];
    for path in invalid {
        let res = manager.create_note(path, &mut notes).await;
        assert!(res.is_err());
    }
}

#[tokio::test]
async fn test_note_creation_weird_characters() {
    let harness = NotebookTestHarness::new("weird_chars");
    let manager = harness.manager();
    let mut notes = Vec::new();

    // Paths containing characters that are generally allowed or dis-allowed
    let weird_paths = vec![
        "note!@#",
        "note_$%^",
        "note_&()_+",
        "note_-=\\\"'`~|", // some of these will fall back to normal components
    ];

    for path in weird_paths {
        let res = manager.create_note(path, &mut notes).await;
        // Weird characters inside component names are allowed unless they violate basic path traversal
        if let Ok(note) = res {
            assert!(!note.rel_path.is_empty());
        }
    }
}

#[tokio::test]
async fn test_attachment_path_traversal_attempts() {
    let temp = TempTestDir::new("attach_traversal");

    // Create a real file outside the notebook directory to ensure canonicalization succeeds but boundary check fails.
    let outside_file = temp
        .path()
        .parent()
        .unwrap()
        .join("cognate_traversal_test.png");
    std::fs::write(&outside_file, "dummy content").unwrap();

    let read_res =
        AttachmentManager::read_image_bytes(temp.path(), "../cognate_traversal_test.png").await;
    assert!(
        read_res.is_err(),
        "Expected reading outside file to fail boundary check"
    );
    assert!(
        matches!(read_res.unwrap_err(), EngineError::Validation { .. }),
        "Expected validation error for reading outside file"
    );

    let delete_res =
        AttachmentManager::delete_attachment(temp.path(), "../cognate_traversal_test.png").await;
    assert!(
        delete_res.is_err(),
        "Expected deleting outside file to fail boundary check"
    );
    assert!(
        matches!(delete_res.unwrap_err(), EngineError::Validation { .. }),
        "Expected validation error for deleting outside file"
    );

    let _ = std::fs::remove_file(&outside_file);
}

#[tokio::test]
async fn test_attachment_invalid_base64_and_corrupt_payloads() {
    let temp = TempTestDir::new("corrupt_attachments");

    // Saving completely invalid base64 (e.g. invalid length/chars)
    let res = AttachmentManager::save_image_from_base64(temp.path(), "note", "invalid_!!!").await;
    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), EngineError::Validation { .. }));
}

#[tokio::test]
async fn test_image_extension_formats() {
    let temp = TempTestDir::new("img_formats");

    let mut notes = Vec::new();
    let manager = NotebookManager::new(temp.path());
    manager.create_note("note", &mut notes).await.unwrap();

    // JPG payload base64: /9j/AA== (decodes to [0xFF, 0xD8, 0xFF, 0x00])
    let jpg_base64 = "/9j/AA==";
    let jpg_path = AttachmentManager::save_image_from_base64(temp.path(), "note", jpg_base64)
        .await
        .unwrap();
    assert!(jpg_path.ends_with(".jpg"));

    // GIF payload base64: R0lGODlh (decodes to b"GIF89a")
    let gif_base64 = "R0lGODlh";
    let gif_path = AttachmentManager::save_image_from_base64(temp.path(), "note", gif_base64)
        .await
        .unwrap();
    assert!(gif_path.ends_with(".gif"));

    // WEBP payload base64: UklGRgAAAABXRUJQ (decodes to b"RIFF\0\0\0\0WEBP")
    let webp_base64 = "UklGRgAAAABXRUJQ";
    let webp_path = AttachmentManager::save_image_from_base64(temp.path(), "note", webp_base64)
        .await
        .unwrap();
    assert!(webp_path.ends_with(".webp"));
}

#[tokio::test]
async fn test_attachment_extension_fallback_and_symlink_save_escape() {
    let temp = TempTestDir::new("attach_fallback_escape");

    let dummy_fallback_base64 = "SGVsbG8=";
    let rel_path =
        AttachmentManager::save_image_from_base64(temp.path(), "note", dummy_fallback_base64)
            .await
            .unwrap();
    assert!(
        rel_path.ends_with(".png"),
        "Expected fallback to png extension, got: {}",
        rel_path
    );

    #[cfg(unix)]
    {
        let outside_dir =
            std::env::temp_dir().join(format!("cognate_outside_attach_{}", now_nanos()));
        std::fs::create_dir_all(outside_dir.join("images")).unwrap();

        let symlink_path = temp.path().join("linked_attach");
        std::os::unix::fs::symlink(&outside_dir, &symlink_path).unwrap();

        let res =
            AttachmentManager::save_image_from_base64(temp.path(), "linked_attach", "SGVsbG8=")
                .await;
        assert!(
            res.is_err(),
            "Expected save image inside symlink resolving outside to fail"
        );
        assert!(
            matches!(res.unwrap_err(), EngineError::Validation { .. }),
            "Expected validation error for image save escape"
        );

        let missing_target_outside =
            std::env::temp_dir().join(format!("cognate_missing_attach_{}", now_nanos()));
        std::fs::create_dir_all(&missing_target_outside).unwrap();
        let missing_link = temp.path().join("missing_link");
        std::os::unix::fs::symlink(&missing_target_outside, &missing_link).unwrap();
        let res =
            AttachmentManager::save_image_from_base64(temp.path(), "missing_link", "SGVsbG8=")
                .await;
        assert!(
            matches!(res, Err(EngineError::Validation { .. })),
            "Expected unresolved outside symlink to be rejected"
        );
        assert!(
            !missing_target_outside.join("images").exists(),
            "Rejected attachment write must not create directories outside the notebook"
        );

        let _ = std::fs::remove_dir_all(&outside_dir);
        let _ = std::fs::remove_dir_all(&missing_target_outside);
    }
}

#[tokio::test]
#[cfg(unix)]
async fn test_attachment_write_storage_error() {
    let temp = TempTestDir::new("attach_write_err");
    let manager = NotebookManager::new(temp.path());
    let mut notes = Vec::new();
    manager.create_note("note", &mut notes).await.unwrap();

    // Make note directory read-only (remove write permissions)
    let note_dir = temp.path().join("note");
    let mut perms = std::fs::metadata(&note_dir).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&note_dir, perms).unwrap();

    // Saving image should fail to create/write inside the read-only directory
    let res = AttachmentManager::save_image_from_base64(temp.path(), "note", "SGVsbG8=").await;
    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), EngineError::Storage { .. }));

    // Restore permissions for cleanup
    let mut perms = std::fs::metadata(&note_dir).unwrap().permissions();
    #[cfg(unix)]
    perms.set_mode(perms.mode() | 0o700);
    #[cfg(not(unix))]
    perms.set_readonly(false);
    let _ = std::fs::set_permissions(&note_dir, perms);
}
