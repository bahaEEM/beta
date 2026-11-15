# SPDX-License-Identifier: AGPL-3.0-or-later
# WebSocket connection. Wire format: JSON -> zstd (see docs/protocol.md).
class_name Net
extends RefCounted

signal opened
signal closed
signal message(msg: Dictionary)

const PROTOCOL_VERSION := 10
const MAX_BYTES := 1048576

var _ws := WebSocketPeer.new()
var _was_open := false


func connect_to(address: String) -> Error:
	_ws = WebSocketPeer.new()
	_ws.inbound_buffer_size = MAX_BYTES * 2
	_was_open = false
	return _ws.connect_to_url("ws://" + address)


func send(msg: Dictionary) -> void:
	if _ws.get_ready_state() != WebSocketPeer.STATE_OPEN:
		return
	var raw := JSON.stringify(msg).to_utf8_buffer()
	_ws.send(raw.compress(FileAccess.COMPRESSION_ZSTD), WebSocketPeer.WRITE_MODE_BINARY)


# Call every frame.
func poll() -> void:
	_ws.poll()
	var state := _ws.get_ready_state()
	if state == WebSocketPeer.STATE_OPEN and not _was_open:
		_was_open = true
		opened.emit()
	while _ws.get_available_packet_count() > 0:
		var packet := _ws.get_packet()
		# decompress_dynamic does not support zstd; the buffer is trimmed to the real size.
		var raw := packet.decompress(MAX_BYTES, FileAccess.COMPRESSION_ZSTD)
		var parsed = JSON.parse_string(raw.get_string_from_utf8())
		if parsed is Dictionary:
			message.emit(parsed)
		else:
			push_warning("bad message from server")
	if state == WebSocketPeer.STATE_CLOSED and _was_open:
		_was_open = false
		closed.emit()
