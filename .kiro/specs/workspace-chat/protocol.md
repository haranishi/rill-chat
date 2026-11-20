# Browser/server contract

API paths start with /api. IDs and Unix timestamps in milliseconds are JSON numbers. Successful collection reads return arrays. Error responses use a non-2xx status with a JSON error string. Use JSON requests with same-origin cookies. Production runs with RILL_ENV=production; RILL_BIND defaults to 127.0.0.1:3847 and RILL_DB to data/rill.sqlite3. GET /healthz is a lightweight health check.

| Method and path | Payload or query | Response |
| --- | --- | --- |
| POST /auth/create | workspace, name, handle, password | set cookie |
| POST /auth/join | code, name, handle, password | set cookie |
| POST /auth/login | handle, password | set cookie |
| POST /auth/logout | {} | clear cookie |
| GET /state | | {me,workspace,channels,members} |
| POST /invites | {} (owner only) | {code,expires_at}, valid for 24 hours |
| POST /channels | name, topic | channel |
| POST /dms | user_id | channel |
| GET /channels/:id/messages | optional parent_id, before (exclusive), around (center on message) | oldest-first page of up to 50 messages |
| POST /channels/:id/messages | body, client_id, optional parent_id | committed message (retry same ID safely) |
| POST /messages/:id/reactions | emoji, active (boolean) | updated message |
| GET /search | q | up to 50 matching accessible messages, newest first |
| GET /events | | SSE ready, message, refresh, resync |

User = {id,name,handle}. Workspace = {id,name,owner_id}. Channel = {id,name,topic,kind,peer_id}, kind channel or dm, peer_id nullable. Message = {id,channel_id,author_id,body,parent_id,created_at,reply_count,reactions}. Reaction = {emoji,count,mine}. Palette: 👍 🎉 ❤️ 👀. The frontend resolves authors from state.members. Accept 1..4000 characters per message, 1..48 for channel names, max 180 for topic, 1..48 for display/workspace name, case-insensitive ASCII handles matching [a-z0-9_-]{3,32}, passwords 10..128 characters. Treat message bodies as text.

SSE message contains the complete committed message on create (new messages have no reactions). For a reply the UI updates the parent's reply count when first seen. refresh contains {} for metadata or {channel_id} for a conversation change (including reactions); it is filtered per viewer. ready and resync trigger an authoritative reload. No credential records or raw secrets are returned by state/search/events. Logout and session expiry terminate access to any open stream.
