# What belongs in Rill

Rill is a team chat that a small group can run on one machine. A member should be able to correct a message, return to a decision weeks later and keep a DM between its participants. The operator should be able to move the whole workspace using one backup.

## Decisions contributors can rely on

| Design choice | What it means when changing Rill |
| --- | --- |
| The team controls its data | Signing in, delivering messages and searching work on the team's host. Introduce an external service only with a documented need, an operator opt-in and a recovery path. |
| Publish after committing | A successful send is durable before another member sees it. Preserve retry keys across edits. Demonstrate crash/reconnect behavior when changing delivery. |
| Put privacy checks beside storage | Reuse the same membership rules in history, search, edits, saved items and event distribution. Test a third account as well as the two people in a DM. |
| Share the application language | Write server behavior and browser interactions in Rust. Cargo compiles the browser crate to WebAssembly and packages its generated browser bindings inside the native executable. HTML and CSS handle layout and presentation. |
| Measure the wait the reader sees | Report sender-to-peer event latency and Enter-to-peer DOM latency, with sample count and hardware. Keep durable commit settings during benchmarks. |
| Carry existing teams forward | Test both an empty database and an upgrade from a released version. Document the matching backup for a downgrade. Keep versioned builds and immutable release receipts available. |

## What we brought over from the Slack workflow

| Everyday task | Rill v0.3 |
| --- | --- |
| Correct a published message | Authors can edit posts and replies. Other readers see the edit, search indexes the new text and an older editor cannot overwrite a newer revision silently. [Slack message editing](https://slack.com/help/articles/202395258-Edit-or-delete-messages) |
| Come back to something privately | Each member has a saved list for channels, DMs and replies. An entry opens the original conversation or thread. [Slack saved items](https://slack.com/help/articles/360042650274-Save-messages-and-files-for-later) |

## Candidates for the next releases

| Gap | Decision needed before implementation |
| --- | --- |
| Mentions and unread notifications | Define what counts as read across devices and which conversations can notify whom. [Slack mentions](https://slack.com/help/articles/205240127-Use-mentions-in-Slack) |
| File attachments | Choose per-workspace quotas, authorized download URLs and how files join the database backup and restore. [Slack files](https://slack.com/help/articles/201330736-Add-files-to-Slack) |
| Message deletion | Agree on how deleted parents, replies, search results, saved entries and backups should behave. [Slack message deletion](https://slack.com/help/articles/202395258-Edit-or-delete-messages) |

Proposals should describe a task a member or operator needs to complete, the effect on access and stored data, and how reviewers can reproduce it. Contributors can use the [requirements](../.kiro/specs/workspace-chat/requirements.md), [design](../.kiro/specs/workspace-chat/design.md) and [HTTP/SSE contract](../.kiro/specs/workspace-chat/protocol.md) to locate the existing guarantees.
