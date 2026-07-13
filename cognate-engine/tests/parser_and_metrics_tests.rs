use cognate_engine::metrics::MetricsRegister;
use cognate_engine::tasks::TaskRegister;

#[test]
fn test_task_extraction_marker_variants() {
    let mut register = TaskRegister::new();

    let content = "\
- [ ] Task 1 (normal bullet, pending)
* [x] Task 2 (asterisk bullet, lowercase completed)
- [X] Task 3 (normal bullet, uppercase completed)
- [ ] 
- Task 4 (no task box, should be ignored)
1. [ ] Task 5 (numbered list, should be ignored)
  - [ ] Task 6 (indented bullet, pending)
";

    register.update_tasks_for_note("notes/work", content);

    let pending = register.get_pending_tasks();
    let all_tasks = register
        .tasks_by_note
        .get("notes/work")
        .expect("Expected note to be indexed");

    // Task 6 is indented. Let's see: lines start with "  - [ ]"
    // Since update_tasks_for_note calls trimmed = line.trim_start(),
    // and then checks starts_with("- ") or starts_with("* "),
    // task 6 should be successfully extracted!
    assert_eq!(all_tasks.len(), 4); // Task 1, 2, 3, 6

    let task_1 = all_tasks
        .iter()
        .find(|t| t.text == "Task 1 (normal bullet, pending)")
        .unwrap();
    assert_eq!(task_1.is_completed, false);
    assert_eq!(task_1.line_number, 1);

    let task_2 = all_tasks
        .iter()
        .find(|t| t.text == "Task 2 (asterisk bullet, lowercase completed)")
        .unwrap();
    assert_eq!(task_2.is_completed, true);
    assert_eq!(task_2.line_number, 2);

    let task_3 = all_tasks
        .iter()
        .find(|t| t.text == "Task 3 (normal bullet, uppercase completed)")
        .unwrap();
    assert_eq!(task_3.is_completed, true);
    assert_eq!(task_3.line_number, 3);

    let task_6 = all_tasks
        .iter()
        .find(|t| t.text == "Task 6 (indented bullet, pending)")
        .unwrap();
    assert_eq!(task_6.is_completed, false);
    assert_eq!(task_6.line_number, 7);

    assert_eq!(pending.len(), 2); // Task 1, Task 6
}

#[test]
fn test_task_extraction_due_date_patterns() {
    let mut register = TaskRegister::new();

    let content = "\
- [ ] Complete code review due: 2026-07-15
- [ ] Submit progress report @due(2026-07-20)
- [ ] Write tests with invalid date due: 2026/07/25
- [ ] Read specs with broken tag @due(26-07-2026)
- [ ] Draft roadmap with short date due: 2026-7-6
- [ ] Clean workspace due: 2026-07-15 extra text
";

    register.update_tasks_for_note("notes/due_dates", content);
    let tasks = register.tasks_by_note.get("notes/due_dates").unwrap();

    // Check Task 1 (due: YYYY-MM-DD)
    let t1 = tasks
        .iter()
        .find(|t| t.text.starts_with("Complete code review"))
        .unwrap();
    assert_eq!(t1.due_date, Some("2026-07-15".to_string()));
    assert_eq!(t1.text, "Complete code review");

    // Check Task 2 (@due(YYYY-MM-DD))
    let t2 = tasks
        .iter()
        .find(|t| t.text.starts_with("Submit progress report"))
        .unwrap();
    assert_eq!(t2.due_date, Some("2026-07-20".to_string()));
    assert_eq!(t2.text, "Submit progress report");

    // Check Task 3 (slash separator, should fail validation and not extract date)
    let t3 = tasks
        .iter()
        .find(|t| t.text.contains("invalid date"))
        .unwrap();
    assert_eq!(t3.due_date, None);
    assert!(t3.text.contains("due: 2026/07/25"));

    // Check Task 4 (DD-MM-YYYY format, should fail YYYY-MM-DD structure check)
    let t4 = tasks
        .iter()
        .find(|t| t.text.contains("broken tag"))
        .unwrap();
    assert_eq!(t4.due_date, None);

    // Check Task 5 (short month/day component, should fail)
    let t5 = tasks
        .iter()
        .find(|t| t.text.contains("short date"))
        .unwrap();
    assert_eq!(t5.due_date, None);

    // Check Task 6 (due date with trailing content, should clean text correctly)
    let t6 = tasks
        .iter()
        .find(|t| t.text.contains("Clean workspace"))
        .unwrap();
    assert_eq!(t6.due_date, Some("2026-07-15".to_string()));
    assert_eq!(t6.text, "Clean workspace  extra text");
}

#[test]
fn test_task_register_updates_and_removal() {
    let mut register = TaskRegister::new();

    // Initial note content
    register.update_tasks_for_note("note-1", "- [ ] First task\n- [ ] Second task");
    assert_eq!(register.tasks_by_note.get("note-1").unwrap().len(), 2);

    // Update note content (removed second task, added third)
    register.update_tasks_for_note("note-1", "- [ ] First task\n- [x] Third task");
    let tasks = register.tasks_by_note.get("note-1").unwrap();
    assert_eq!(tasks.len(), 2);
    assert!(
        tasks
            .iter()
            .any(|t| t.text == "First task" && !t.is_completed)
    );
    assert!(
        tasks
            .iter()
            .any(|t| t.text == "Third task" && t.is_completed)
    );

    // Update to content without any tasks (should remove note entry completely)
    register.update_tasks_for_note("note-1", "Just plain text notes now.");
    assert!(!register.tasks_by_note.contains_key("note-1"));

    // Verify remove_note deletes entry directly
    register.update_tasks_for_note("note-2", "- [ ] Temp task");
    assert!(register.tasks_by_note.contains_key("note-2"));
    register.remove_note("note-2");
    assert!(!register.tasks_by_note.contains_key("note-2"));
}

#[test]
fn test_metrics_empty_or_whitespace_documents() {
    let mut register = MetricsRegister::new();

    // Empty doc
    register.update_metrics_for_note("empty", "");
    assert!(!register.metrics_by_note.contains_key("empty"));

    // Whitespace doc
    register.update_metrics_for_note("whitespace", "   \n\t   ");
    assert!(!register.metrics_by_note.contains_key("whitespace"));

    // Non-alphanumeric doc (punctuation only)
    register.update_metrics_for_note("symbols", "!@#$%^&*()_+");
    assert!(!register.metrics_by_note.contains_key("symbols"));
}

#[test]
fn test_metrics_sentence_counting_boundaries() {
    let mut register = MetricsRegister::new();

    // Doc with standard periods, exclamations, question marks
    let doc1 = "Hello world! How are you today? This is a test note.";
    register.update_metrics_for_note("doc1", doc1);
    let m1 = register.metrics_by_note.get("doc1").unwrap();
    assert_eq!(m1.sentence_count, 3);
    assert_eq!(m1.word_count, 11);

    // Period inside a word or abbreviation (should not treat as sentence boundaries)
    let doc2 = "Meet me at 10:00 a.m. at the U.S. embassy. Bring the file.txt document.";
    register.update_metrics_for_note("doc2", doc2);
    let m2 = register.metrics_by_note.get("doc2").unwrap();
    assert_eq!(m2.sentence_count, 4);
}

#[test]
fn test_metrics_syllable_counting_rules() {
    let mut register = MetricsRegister::new();

    // Testing silent 'e' at end
    register.update_metrics_for_note("m1", "make code");
    let metrics1 = register.metrics_by_note.get("m1").unwrap();
    assert_eq!(metrics1.word_count, 2);

    // Testing vowel groups
    register.update_metrics_for_note("m2", "beautiful queen");
    let metrics2 = register.metrics_by_note.get("m2").unwrap();
    assert_eq!(metrics2.word_count, 2);

    // Testing short words
    register.update_metrics_for_note("m3", "a me");
    let metrics3 = register.metrics_by_note.get("m3").unwrap();
    assert_eq!(metrics3.word_count, 2);
}
