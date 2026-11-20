# Browser/server contract

API paths start with /api. IDs and Unix timestamps in milliseconds are JSON numbers. Successful collection reads return arrays. Error responses use a non-2xx status with a JSON error string. Use JSON requests with same-origin cookies. Production runs with RILL_ENV=production; RILL_BIND defaults to 127.0.0.1:3847 and RILL_DB to data/rill.sqlite3. GET /healthz is a lightweight health check.

| Method and path | Payload or query | Response |
| --- | --- | --- |
| POST /auth/create | workspace, name, handle, password | set cookie |
| POST /auth/join | code, name, handle, password | set cookie |
| POST /auth/login | handle, password | set cookie |
| POST /auth/password | current_password, new_password | revoke all sessions for the member and set a new cookie |
| POST /auth/logout | {} | clear cookie |
| GET /state | | {me,workspace,channels,members} |
| POST /invites | {} (owner only) | {code,expires_at}, valid for 24 hours |
| POST /channels | name, topic | channel |
| POST /dms | user_id | channel |
| GET /channels/:id/messages | optional parent_id, before (exclusive), around (center on message) | oldest-first page of up to 50 messages |
| POST /channels/:id/messages | body, client_id, optional parent_id | committed message (retry same ID safely) |
| POST /messages/:id/reactions | emoji, active (boolean) | updated message |
| POST /messages/:id/edit | body, expected_edited_at (previous timestamp or null) | author-only edit, updated message or 409 if superseded |
| POST /messages/:id/saved | active (boolean) | updated message with the caller's saved state |
| GET /saved | optional before (exclusive message ID) | up to 50 accessible saved messages, newest ID first |
| GET /search | q | up to 50 matching accessible messages, newest first |
| GET /events | | SSE ready, message, refresh, resync |

User = {id,name,handle}. Workspace = {id,name,owner_id}. Channel = {id,name,topic,kind,peer_id}, kind channel or dm, peer_id nullable. Message = {id,channel_id,author_id,body,parent_id,created_at,edited_at,saved,reply_count,reactions}. The initial edited_at is null. A read returns saved for the authenticated reader. Reaction = {emoji,count,mine}. Palette: 👍 🎉 ❤️ 👀. The frontend resolves authors from state.members. Accept 1..4000 characters per message, 1..48 for channel names, max 180 for topic, 1..48 for display/workspace name, case-insensitive ASCII handles matching [a-z0-9_-]{3,32}, passwords 10..128 characters. Treat message bodies as text.

SSE message contains the complete committed message on create. For a reply the UI updates the parent's reply count when first seen. A refresh carries {} for metadata, {channel_id} for conversation changes, or {channel_id,message_id} for edits. Saving sends {channel_id,message_id,saved_changed:true} only to the member whose list changed. A ready or resync event triggers an authoritative reload. Logout, password rotation and expiry terminate streams tied to invalid sessions.
