do
	local fizyr_rpc_tcp_proto = Proto("fizyr_rpc_tcp", "Fizyr RPC over TCP");
	local fizyr_rpc_udp_proto = Proto("fizyr_rpc_udp", "Fizyr RPC over UDP");

	local udp_length = Field.new("udp.length");

	local field_length = ProtoField.uint32(
		"fizyr_rpc.message_length",
		"Message length",
		base.DEC,
		nil,
		nil,
		"The length of the message, excluding the message length itself."
	)

	local field_kind = ProtoField.uint32(
		"fizyr_rpc.message_type",
		"Message type",
		base.DEC,
		{
			[0] = "Request",
			[1] = "Response",
			[2] = "RequestUpdate",
			[3] = "ResponseUpdate",
			[4] = "Stream",
		},
		nil,
		"The type of the RPC message: Request, Response, RequestUpdate, ResponseUpdate or Stream."
	)

	local field_request_id = ProtoField.uint32(
		"fizyr_rpc.request_id",
		"Request ID",
		base.DEC,
		nil,
		nil,
		"The request ID of the message. Not used for stream messages."
	)

	local field_service_id = ProtoField.int32(
		"fizyr_rpc.service_id",
		"Service ID",
		base.DEC,
		nil,
		nil,
		"The service ID of the message."
	)

	local field_body = ProtoField.bytes(
		"fizyr_rpc.body",
		"Message body",
		base.SPACE,
		"The message body/payload."
	)

	fizyr_rpc_tcp_proto.fields = {
		field_length,
		field_kind,
		field_request_id,
		field_service_id,
		field_body,
	}

	function fizyr_rpc_tcp_proto.dissector(buffer, pinfo, tree)
		dissect_tcp_pdus(buffer, tree, 4, fizyr_rpc_tcp_get_length, fizyr_rpc_tcp_dissect_reassembled)
	end

	function fizyr_rpc_tcp_get_length(buffer, pinfo, offset)
		return 4 + buffer(0, 4):le_uint()
	end

	function fizyr_rpc_tcp_dissect_reassembled(buffer, pinfo, tree)
		local subtree = tree:add(fizyr_rpc_tcp_proto, buffer)
		subtree:add_le(field_length,     buffer(0, 4))
		subtree:add_le(field_kind,       buffer(4, 4))
		subtree:add_le(field_request_id, buffer(8, 4))
		subtree:add_le(field_service_id, buffer(12, 4))
		subtree:add_le(field_body, buffer(16))
	end

	function fizyr_rpc_udp_proto.dissector(buffer, pinfo, tree)
		subtree:add_le(field_length,     udp_length())
		subtree:add_le(field_kind,       buffer(0, 4))
		subtree:add_le(field_request_id, buffer(4, 4))
		subtree:add_le(field_service_id, buffer(8, 4))
		subtree:add_le(field_body, buffer(12))
	end

	local tcp_table = DissectorTable.get("tcp.port")
	tcp_table:add("1-65535", fizyr_rpc_tcp_proto)

	local udp_table = DissectorTable.get("udp.port")
	udp_table:add("1-65535", fizyr_rpc_udp_proto)
end
