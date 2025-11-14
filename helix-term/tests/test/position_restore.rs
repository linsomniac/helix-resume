/// AIDEV-NOTE: Integration tests for position restoration feature.
/// Tests both the fix for redraw issues and the multiple buffer workflow.
///
/// AIDEV-TODO: These tests require save_file_info to be enabled in the config.
/// Currently, the AppBuilder doesn't easily support setting this config option.
/// The tests are written but need the test infrastructure to be updated to support
/// enabling the save_file_info feature flag for testing.

use super::*;
use helix_stdx::path;
use helix_view::current_ref;
use std::fs;
use std::io::Write;

#[tokio::test(flavor = "multi_thread")]
async fn test_position_restore_on_reopen() -> anyhow::Result<()> {
    // Test that position is restored when reopening a file
    let mut file = tempfile::NamedTempFile::new()?;

    // Create a file with multiple lines
    file.write_all(b"line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\n")?;
    file.flush()?;

    let mut app = helpers::AppBuilder::new()
        .with_file(file.path(), None)
        .build()?;

    // Move to line 5 and save position by closing
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("5g"),  // Go to line 5
                Some(&|app| {
                    let (view, doc) = current_ref!(app.editor);
                    let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
                    let line = doc.text().char_to_line(cursor);
                    assert_eq!(line, 4); // Line 5 is index 4
                }),
            ),
        ],
        false,
    )
    .await?;

    // Close the editor (this should save the position)
    app.editor.close(app.editor.tree.focus);

    // Create a new app and reopen the same file
    let mut app2 = helpers::AppBuilder::new()
        .with_file(file.path(), None)
        .build()?;

    // Check that position was restored
    helpers::run_event_loop_until_idle(&mut app2).await;

    let (view, doc) = current_ref!(app2.editor);
    let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
    let line = doc.text().char_to_line(cursor);

    // Should be restored to line 5 (index 4)
    assert_eq!(line, 4, "Position should be restored to line 5");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiple_views_same_file_no_jump() -> anyhow::Result<()> {
    // Test that multiple views of the same file maintain independent positions
    let mut file = tempfile::NamedTempFile::new()?;

    // Create a file with multiple lines
    file.write_all(b"line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\n")?;
    file.flush()?;

    let mut app = helpers::AppBuilder::new()
        .with_file(file.path(), None)
        .build()?;

    test_key_sequences(
        &mut app,
        vec![
            (
                Some("5g"),  // Go to line 5 in first view
                Some(&|app| {
                    let (view, doc) = current_ref!(app.editor);
                    let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
                    let line = doc.text().char_to_line(cursor);
                    assert_eq!(line, 4); // Line 5 is index 4
                }),
            ),
            (
                Some(":vsplit<ret>"),  // Create vertical split
                Some(&|app| {
                    assert_eq!(2, app.editor.tree.views().count());
                }),
            ),
            (
                Some("10g"),  // Go to line 10 in second view
                Some(&|app| {
                    let (view, doc) = current_ref!(app.editor);
                    let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
                    let line = doc.text().char_to_line(cursor);
                    assert_eq!(line, 9); // Line 10 is index 9
                }),
            ),
            (
                Some("<C-w>w"),  // Switch back to first view
                Some(&|app| {
                    let (view, doc) = current_ref!(app.editor);
                    let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
                    let line = doc.text().char_to_line(cursor);
                    // Should still be at line 5, not jumped to line 10
                    assert_eq!(line, 4, "First view should maintain position at line 5");
                }),
            ),
            (
                Some("<C-w>w"),  // Switch to second view again
                Some(&|app| {
                    let (view, doc) = current_ref!(app.editor);
                    let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
                    let line = doc.text().char_to_line(cursor);
                    // Should still be at line 10
                    assert_eq!(line, 9, "Second view should maintain position at line 10");
                }),
            ),
        ],
        false,
    )
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_open_file_already_in_view_no_restore() -> anyhow::Result<()> {
    // Test that opening a file already open in another view doesn't restore position
    let mut file1 = tempfile::NamedTempFile::new()?;
    let mut file2 = tempfile::NamedTempFile::new()?;

    // Create files with multiple lines
    file1.write_all(b"file1 line 1\nfile1 line 2\nfile1 line 3\nfile1 line 4\nfile1 line 5\n")?;
    file1.flush()?;

    file2.write_all(b"file2 line 1\nfile2 line 2\nfile2 line 3\n")?;
    file2.flush()?;

    let mut app = helpers::AppBuilder::new()
        .with_file(file1.path(), None)
        .build()?;

    test_key_sequences(
        &mut app,
        vec![
            (
                Some("3g"),  // Go to line 3 in file1
                Some(&|app| {
                    let (view, doc) = current_ref!(app.editor);
                    let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
                    let line = doc.text().char_to_line(cursor);
                    assert_eq!(line, 2); // Line 3 is index 2
                }),
            ),
            (
                Some(&format!(":o {}<ret>", file2.path().to_string_lossy())),  // Open file2
                Some(&|app| {
                    let (view, doc) = current_ref!(app.editor);
                    assert_eq!(doc.path().unwrap(), &path::normalize(file2.path()));
                }),
            ),
            (
                Some(&format!(":o {}<ret>", file1.path().to_string_lossy())),  // Re-open file1 in same view
                Some(&|app| {
                    let (view, doc) = current_ref!(app.editor);
                    let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
                    let line = doc.text().char_to_line(cursor);
                    // Should be at line 1 (not restored to line 3) since file1 is already open in another view
                    assert_eq!(line, 0, "Should not restore position when file is already open in another view");
                }),
            ),
        ],
        false,
    )
    .await?;

    Ok(())
}