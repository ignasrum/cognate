# Manual Testing Checklist

Use this checklist before releases or significant UI changes.

## Startup and Configuration

- App starts with valid `config.json`
- App fails gracefully with invalid config
- `COGNATE_CONFIG_PATH` override works
- UI scale changes persist across restart

## Note Lifecycle

- Create note in root and nested path
- Delete selected note and verify explorer refresh
- Move/rename a note and ensure selection remains valid
- Move/rename folder and verify nested note paths update correctly

## Editing and Preview

- Typing updates note content and preview
- Undo/redo behave correctly for sequential edits
- Tab key and select-all shortcuts behave as expected
- Preview cursor indicator tracks selection reasonably

## Labels and Search

- Add/remove labels updates UI and persists metadata
- Search matches path, label, and content
- Clearing search restores normal explorer state

## Embedded Images

- Paste image from clipboard/file URI path
- Image renders in preview
- Deleting image reference prompts and handles file cleanup

## Visualizer

- Toggle visualizer and return to editor
- Focus node and double-click open note flow works
- Graph updates after label edits or note changes

## Shutdown and Recovery

- Closing window attempts save and exits cleanly
- Simulate failing write paths and verify error dialogs
- Reopen app and verify latest note/metadata state

