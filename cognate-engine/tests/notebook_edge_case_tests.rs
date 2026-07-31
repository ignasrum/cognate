mod common;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use cognate_engine::EngineError;
use cognate_engine::storage::{NoteMetadata, NotebookManager};
use common::{TempTestDir, now_nanos};

#[test]
fn test_direct_struct_invocations_for_coverage() {
    let state1 = cognate_engine::NotebookEngineState::new();
    let state2 = state1.clone();
    assert_eq!(state1, state2);
    let _ = format!("{:?}", state1);
    let err1 = EngineError::validation("context", "detail");
    let err2 = err1.clone();
    assert_eq!(err1, err2);
    let _ = format!("{:?}", err1);
    let index1 = cognate_engine::search::InvertedIndex::new();
    let _ = index1.clone();
    let load_res = cognate_engine::NotebookEngineState::load_from_bytes(&[]);
    assert!(matches!(
        load_res.unwrap_err(),
        EngineError::Deserialization(_)
    ));
}

#[tokio::test]
async fn test_notebook_manager_misc_coverage() {
    let temp = TempTestDir::new("misc_cov");
    let manager = NotebookManager::new(temp.path());
    assert_eq!(manager.notebook_path(), temp.path());
}

#[tokio::test]
async fn test_metadata_overwrite_invalid_fails() {
    let temp = TempTestDir::new("metadata_invalid_fails");
    let manager = NotebookManager::new(temp.path());
    std::fs::write(temp.path().join("metadata.json"), "{ invalid JSON }").unwrap();
    let notes = vec![NoteMetadata {
        rel_path: "note-1".to_string(),
        labels: vec![],
        last_updated: None,
    }];
    let result = manager.save_metadata(&notes).await;
    assert!(matches!(result.unwrap_err(), EngineError::Recovery { .. }));
}

#[tokio::test]
async fn test_delete_note_parent_not_empty() {
    let temp = TempTestDir::new("delete_parent_not_empty");
    let manager = NotebookManager::new(temp.path());
    let mut notes = Vec::new();
    manager
        .create_note("folder/note", &mut notes)
        .await
        .unwrap();
    let other_file = temp.path().join("folder/another.txt");
    std::fs::write(&other_file, "content").unwrap();
    manager
        .delete_note("folder/note", &mut notes)
        .await
        .unwrap();
    assert!(temp.path().join("folder").exists());
    assert!(other_file.exists());
}

#[tokio::test]
async fn test_delete_note_not_found_on_disk() {
    let temp = TempTestDir::new("delete_not_found");
    let manager = NotebookManager::new(temp.path());
    let mut notes = Vec::new();
    let result = manager
        .delete_note("non_existent_folder/file", &mut notes)
        .await;
    assert!(matches!(
        result.unwrap_err(),
        EngineError::Validation { .. }
    ));
}

#[tokio::test]
async fn test_move_note_source_not_found() {
    let temp = TempTestDir::new("move_source_not_found");
    let manager = NotebookManager::new(temp.path());
    let mut notes = Vec::new();
    let result = manager
        .move_note("non_existent", "target", &mut notes)
        .await;
    assert!(matches!(
        result.unwrap_err(),
        EngineError::Validation { .. }
    ));
}

#[tokio::test]
#[cfg(unix)]
async fn test_load_metadata_persist_normalization_warning() {
    let temp = TempTestDir::new("norm_warn");
    let manager = NotebookManager::new(temp.path());
    let notes = vec![NoteMetadata {
        rel_path: "note-1".to_string(),
        labels: vec![],
        last_updated: Some("2026-07-13T12:00:00.123456Z".to_string()),
    }];
    manager.save_metadata(&notes).await.unwrap();
    let mut perms = std::fs::metadata(temp.path()).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(temp.path(), perms).unwrap();
    let load_result = manager.load_metadata().await.unwrap();
    assert!(
        load_result
            .warning
            .unwrap()
            .contains("failed to persist normalized timestamps")
    );
    let mut perms = std::fs::metadata(temp.path()).unwrap().permissions();
    perms.set_mode(perms.mode() | 0o700);
    let _ = std::fs::set_permissions(temp.path(), perms);
}

#[tokio::test]
async fn test_move_note_target_exists_on_disk_only() {
    let temp = TempTestDir::new("move_target_disk_only");
    let manager = NotebookManager::new(temp.path());
    let mut notes = Vec::new();
    manager.create_note("note1", &mut notes).await.unwrap();
    let target_dir = temp.path().join("note2");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("note.md"), "target note").unwrap();
    let error = manager
        .move_note("note1", "note2", &mut notes)
        .await
        .unwrap_err();
    assert!(matches!(error, EngineError::Validation { .. }));
    assert!(
        error
            .to_string()
            .contains("already exists at the target path")
    );
}

#[tokio::test]
#[cfg(unix)]
async fn test_move_note_source_symbolic_link_escape() {
    let temp = TempTestDir::new("move_source_symlink_escape");
    let manager = NotebookManager::new(temp.path());
    let mut notes = vec![NoteMetadata {
        rel_path: "linked_move".to_string(),
        labels: Vec::new(),
        last_updated: None,
    }];
    let outside_dir = std::env::temp_dir().join(format!("cognate_outside_move_{}", now_nanos()));
    std::fs::create_dir_all(&outside_dir).unwrap();
    std::os::unix::fs::symlink(&outside_dir, temp.path().join("linked_move")).unwrap();
    let result = manager.move_note("linked_move", "target", &mut notes).await;
    assert!(matches!(
        result.unwrap_err(),
        EngineError::Validation { .. }
    ));
    let _ = std::fs::remove_dir_all(&outside_dir);
}
