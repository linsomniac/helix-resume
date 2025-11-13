use helix_core::{Selection, Transaction, SmartString, indent};
use helix_event::register_hook;
use helix_view::handlers::Handlers;
use helix_core::chars::char_is_whitespace;

use crate::events::PostInsertChar;

pub(super) fn register_hooks(_handlers: &Handlers) {
    register_hook!(move |event: &mut PostInsertChar<'_, '_>| {
        let config = event.cx.editor.config();

        // Only proceed if wrap_when_typing is enabled
        if !config.wrap_when_typing {
            return Ok(());
        }

        // Get config info we'll need for indentation before the mutable borrow
        let indent_heuristic = config.indent_heuristic.clone();
        let loader = event.cx.editor.syn_loader.load();

        let (view, doc) = current!(event.cx.editor);
        let text_width = doc.text_width();

        // Get information about what lines need wrapping
        let mut wrap_positions = Vec::new();
        {
            let text = doc.text();
            let selection = doc.selection(view.id);

            // Process each cursor position
            for range in selection.ranges() {
                let cursor_pos = range.cursor(text.slice(..));
                let line_idx = text.char_to_line(cursor_pos);
                let line_start = text.line_to_char(line_idx);
                let line_end = if line_idx == text.len_lines() - 1 {
                    text.len_chars()
                } else {
                    text.line_to_char(line_idx + 1) - 1  // Exclude newline
                };

                let line = text.slice(line_start..line_end);
                let line_str = line.to_string();

                // Check if line exceeds text_width
                if line_str.chars().count() > text_width && text_width > 0 {
                    // Find the last whitespace before or at text_width
                    let mut last_space_before_width = None;
                    let mut first_space_after_width = None;
                    let mut char_count = 0;

                    for (idx, ch) in line_str.char_indices() {
                        char_count += 1;
                        if char_is_whitespace(ch) {
                            if char_count <= text_width {
                                last_space_before_width = Some(idx);
                            } else if first_space_after_width.is_none() {
                                // Found first space after text_width
                                first_space_after_width = Some(idx);
                                break; // We can stop searching
                            }
                        }
                    }

                    // Determine which space to use for wrapping
                    let space_idx = if last_space_before_width.is_some() {
                        last_space_before_width
                    } else {
                        // No space before width limit, use first space after (if any)
                        // This handles the case where a single word exceeds text_width
                        first_space_after_width
                    };

                    // If we found a space to break at
                    if let Some(space_idx) = space_idx {
                        // Calculate the position in the document
                        let break_pos = line_start + line_str[..space_idx].chars().count();

                        // Skip any trailing whitespace after the break point
                        let mut next_char_pos = break_pos + 1;
                        while next_char_pos < line_end {
                            let ch = text.char(next_char_pos);
                            if !char_is_whitespace(ch) {
                                break;
                            }
                            next_char_pos += 1;
                        }

                        wrap_positions.push((break_pos, next_char_pos));
                    }
                }
            }
        }

        // Apply wrapping for all positions we found
        for (break_pos, next_char_pos) in wrap_positions {
            let text = doc.text();

            // Calculate the indentation for the new line
            let line_idx = text.char_to_line(break_pos);
            let indent_str = indent::indent_for_newline(
                &loader,
                doc.syntax(),
                &indent_heuristic,
                &doc.indent_style,
                doc.tab_width(),
                text.slice(..),
                line_idx,
                break_pos,
                line_idx,
            );

            // Create the new text with newline + indentation
            let mut new_text = String::from("\n");
            new_text.push_str(&indent_str);

            let transaction = Transaction::change_by_selection(text, &Selection::single(break_pos, next_char_pos), |range| {
                (range.from(), range.to(), Some(SmartString::from(new_text.as_str())))
            });
            doc.apply(&transaction, view.id);
        }

        Ok(())
    });
}