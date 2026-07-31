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
- Search filters work: `label:`, `path:`, `updated:FROM..TO`, quoted phrases, and `-` exclusions
- Rapid typing waits briefly before searching and never displays stale results
- Embedded-local and remote API search return the same matches and match types
- API search pagination returns stable, non-duplicated pages
- Legacy `/v1/search` and paginated `/v1/search/page` responses both work
- Search matches are visually highlighted, including Unicode text
- Search failures are distinct from a valid empty result and offer retry when appropriate
- Invalid paginated searches return a machine-readable `invalid_query` or `invalid_cursor` error
- Editing, moving, deleting, or relabeling a note updates search results immediately
- Clearing search restores normal explorer state

## API Runtime and Security

- Embedded local mode starts on loopback without creating SQLite or credential files
- Embedded API startup failures identify the failed operation and remain visible in the UI
- Embedded local admin client routes are unavailable
- Remote API admin client provisioning and revocation continue to work
- Closing the UI flushes pending writes before the embedded API shuts down
- The embedded API port can be rebound after shutdown

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
