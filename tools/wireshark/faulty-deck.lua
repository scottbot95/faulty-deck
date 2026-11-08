-- GENERATED DO NOT EDIT
-- Generated crates/protocol by `just host::wireshark`
-- See crates/protocol/src/wireshark/template.lua for the template

local proto = Proto("faultydeck", "faulty-deck HID protocol")

local fields = {
    ["command"] = ProtoField.uint32("faultydeck.command", "Command", base.DEC, { [0] = "Sync", [1] = "EnterBootloader", [2] = "UpdateBegin", [3] = "UpdateChunk", [4] = "UpdateCommit", [5] = "UpdateAbort", [6] = "ClearUpdateResult", [7] = "Reboot" }),
    ["command.enter_bootloader"] = ProtoField.bytes("faultydeck.command.enter_bootloader", "EnterBootloader"),
    ["command.update_begin.size"] = ProtoField.uint32("faultydeck.command.update_begin.size", "size"),
    ["command.update_begin.sha256"] = ProtoField.bytes("faultydeck.command.update_begin.sha256", "sha256"),
    ["command.update_begin.version"] = ProtoField.string("faultydeck.command.update_begin.version", "version"),
    ["command.update_chunk.offset"] = ProtoField.uint32("faultydeck.command.update_chunk.offset", "offset"),
    ["command.update_chunk.data"] = ProtoField.bytes("faultydeck.command.update_chunk.data", "data"),
    ["command.reboot"] = ProtoField.bytes("faultydeck.command.reboot", "Reboot"),
    ["event"] = ProtoField.uint32("faultydeck.event", "Event", base.DEC, { [0] = "State", [1] = "Update" }),
    ["event.state.buttons"] = ProtoField.uint16("faultydeck.event.state.buttons", "buttons"),
    ["event.state.sliders"] = ProtoField.uint16("faultydeck.event.state.sliders", "sliders"),
    ["event.update"] = ProtoField.uint32("faultydeck.event.update", "Update", base.DEC, { [0] = "Ack", [1] = "Nak", [2] = "Committed" }),
    ["event.update.ack.offset"] = ProtoField.uint32("faultydeck.event.update.ack.offset", "offset"),
    ["event.update.nak.expected_offset"] = ProtoField.uint32("faultydeck.event.update.nak.expected_offset", "expected_offset"),
    ["event.update.nak.error"] = ProtoField.uint32("faultydeck.event.update.nak.error", "error", base.DEC, { [0] = "NotStarted", [1] = "AlreadyStarted", [2] = "TooLarge", [3] = "BadOffset", [4] = "SizeMismatch", [5] = "BadImage", [6] = "HashMismatch", [7] = "Flash", [8] = "NoSlot", [9] = "NotConfirmed" }),
    ["device_info.protocol_version"] = ProtoField.uint16("faultydeck.device_info.protocol_version", "protocol_version"),
    ["device_info.firmware_version"] = ProtoField.string("faultydeck.device_info.firmware_version", "firmware_version"),
    ["device_info.buttons"] = ProtoField.uint8("faultydeck.device_info.buttons", "buttons"),
    ["device_info.sliders"] = ProtoField.uint8("faultydeck.device_info.sliders", "sliders"),
    ["device_info.serial"] = ProtoField.bytes("faultydeck.device_info.serial", "serial"),
    ["device_info.running_slot"] = ProtoField.uint32("faultydeck.device_info.running_slot", "running_slot", base.DEC, { [0] = "Factory", [1] = "Ota0", [2] = "Ota1" }),
    ["device_info.last_update_result"] = ProtoField.uint32("faultydeck.device_info.last_update_result", "last_update_result", base.DEC, { [0] = "None", [1] = "Aborted", [2] = "Invalid" }),
    ["device_info.failed_version"] = ProtoField.string("faultydeck.device_info.failed_version", "failed_version"),
}

local SCHEMA = {
    Command = {
        k = "enum",
        field = "command",
        variants = {
            [0] = { name = "Sync" },
            [1] = {
                name = "EnterBootloader",
                node = { k = "bytes", n = 4, field = "command.enter_bootloader" },
            },
            [2] = {
                name = "UpdateBegin",
                node = {
                    k = "struct",
                    fields = {
                        { name = "size", node = { k = "varint", ty = "u32", field = "command.update_begin.size" } },
                        { name = "sha256", node = { k = "bytes", n = 32, field = "command.update_begin.sha256" } },
                        { name = "version", node = { k = "string", field = "command.update_begin.version" } },
                    },
                },
            },
            [3] = {
                name = "UpdateChunk",
                node = {
                    k = "struct",
                    fields = {
                        { name = "offset", node = { k = "varint", ty = "u32", field = "command.update_chunk.offset" } },
                        { name = "data", node = { k = "seq", of = { k = "u8" }, field = "command.update_chunk.data" } },
                    },
                },
            },
            [4] = { name = "UpdateCommit" },
            [5] = { name = "UpdateAbort" },
            [6] = { name = "ClearUpdateResult" },
            [7] = {
                name = "Reboot",
                node = { k = "bytes", n = 4, field = "command.reboot" },
            },
        },
    },
    Event = {
        k = "enum",
        field = "event",
        variants = {
            [0] = {
                name = "State",
                node = {
                    k = "struct",
                    fields = {
                        { name = "buttons", node = { k = "varint", ty = "u16", field = "event.state.buttons" } },
                        { name = "sliders", node = {
                            k = "tuple",
                            items = {
                                { k = "varint", ty = "u16", field = "event.state.sliders" },
                                { k = "varint", ty = "u16", field = "event.state.sliders" },
                                { k = "varint", ty = "u16", field = "event.state.sliders" },
                                { k = "varint", ty = "u16", field = "event.state.sliders" },
                            },
                        } },
                    },
                },
            },
            [1] = {
                name = "Update",
                node = {
                    k = "enum",
                    field = "event.update",
                    variants = {
                        [0] = {
                            name = "Ack",
                            node = {
                                k = "struct",
                                fields = {
                                    { name = "offset", node = { k = "varint", ty = "u32", field = "event.update.ack.offset" } },
                                },
                            },
                        },
                        [1] = {
                            name = "Nak",
                            node = {
                                k = "struct",
                                fields = {
                                    { name = "expected_offset", node = { k = "varint", ty = "u32", field = "event.update.nak.expected_offset" } },
                                    { name = "error", node = {
                                        k = "enum",
                                        field = "event.update.nak.error",
                                        variants = {
                                            [0] = { name = "NotStarted" },
                                            [1] = { name = "AlreadyStarted" },
                                            [2] = { name = "TooLarge" },
                                            [3] = { name = "BadOffset" },
                                            [4] = { name = "SizeMismatch" },
                                            [5] = { name = "BadImage" },
                                            [6] = { name = "HashMismatch" },
                                            [7] = { name = "Flash" },
                                            [8] = { name = "NoSlot" },
                                            [9] = { name = "NotConfirmed" },
                                        },
                                    } },
                                },
                            },
                        },
                        [2] = { name = "Committed" },
                    },
                },
            },
        },
    },
    DeviceInfo = {
        k = "struct",
        fields = {
            { name = "protocol_version", node = { k = "varint", ty = "u16", field = "device_info.protocol_version" } },
            { name = "firmware_version", node = { k = "string", field = "device_info.firmware_version" } },
            { name = "buttons", node = { k = "u8", field = "device_info.buttons" } },
            { name = "sliders", node = { k = "u8", field = "device_info.sliders" } },
            { name = "serial", node = {
                k = "newtype",
                of = { k = "bytes", n = 6, field = "device_info.serial" },
            } },
            { name = "running_slot", node = {
                k = "option",
                of = {
                    k = "enum",
                    field = "device_info.running_slot",
                    variants = {
                        [0] = { name = "Factory" },
                        [1] = { name = "Ota0" },
                        [2] = { name = "Ota1" },
                    },
                },
            } },
            { name = "last_update_result", node = {
                k = "enum",
                field = "device_info.last_update_result",
                variants = {
                    [0] = { name = "None" },
                    [1] = { name = "Aborted" },
                    [2] = { name = "Invalid" },
                },
            } },
            { name = "failed_version", node = {
                k = "option",
                of = { k = "string", field = "device_info.failed_version" },
            } },
        },
    },
}

fields.padding = ProtoField.bytes("faultydeck.padding", "Padding")
proto.fields = fields

local ef_malformed = ProtoExpert.new("faultydeck.malformed", "Undecodable report",
    expert.group.MALFORMED, expert.severity.ERROR)
proto.experts = { ef_malformed }

proto.prefs.heuristic = Pref.bool("Decode 64-byte interrupt transfers of unidentified devices", false,
    "Use when the capture lacks the device enumeration, so Wireshark does not know the VID:PID.")

local Reader = {}
Reader.__index = Reader

local function reader(tvb)
    return setmetatable({ tvb = tvb, pos = 0, names = {}, values = {} }, Reader)
end

function Reader:bytes(n)
    local range = self.tvb(self.pos, n)
    self.pos = self.pos + n
    return range
end

function Reader:varint()
    local start, value, mult, b = self.pos, 0, 1, nil
    repeat
        if self.pos - start >= 10 then error("varint longer than 10 bytes") end
        b = self:bytes(1):uint()
        value = value + (b % 128) * mult
        mult = mult * 128
    until b < 128
    return value, self.tvb(start, self.pos - start)
end

function Reader:len_prefixed()
    local n = self:varint()
    return self:bytes(n)
end

local function is_leaf(node)
    while node.k == "newtype" do node = node.of end
    return node.k ~= "struct" and node.k ~= "tuple" and node.k ~= "option" and node.k ~= "unit"
        and not (node.k == "seq" and node.of.k ~= "u8")
end

local function open(tree, r, label)
    if not label then return tree, function() end end
    local start = r.pos
    local sub = tree:add(r.tvb(start), label)
    return sub, function() sub:set_len(r.pos - start) end
end

local function decode_leaf(node, r, tree)
    local k, field = node.k, fields[node.field]
    if k == "varint" then
        local value, range = r:varint()
        if node.zigzag then
            value = value % 2 == 0 and math.floor(value / 2) or -math.floor((value + 1) / 2)
        end
        return tree:add(field, range, value), value
    elseif k == "u8" or k == "i8" or k == "bool" then
        local range = r:bytes(1)
        local value = k == "i8" and range:int() or range:uint()
        if k == "bool" then
            value = value ~= 0
        end
        return tree:add(field, range, value), value
    elseif k == "string" then
        local range = r:len_prefixed()
        return tree:add(field, range, range:string()), range:string()
    elseif k == "bytes" then
        return tree:add(field, r:bytes(node.n))
    elseif k == "seq" then
        return tree:add(field, r:len_prefixed())
    end
    error("unknown schema node " .. tostring(k))
end

local function decode(node, r, tree, label)
    local k = node.k
    if k == "newtype" then
        return decode(node.of, r, tree, label)
    elseif k == "unit" then
        return nil
    elseif k == "enum" then
        local tag, range = r:varint()
        local item = tree:add(fields[node.field], range, tag)
        local variant = node.variants[tag]
        if not variant then error("unknown variant tag " .. tag) end
        r.names[#r.names + 1] = variant.name
        if variant.node then decode(variant.node, r, tree) end
        return item
    elseif k == "struct" then
        local sub, close = open(tree, r, label)
        for _, f in ipairs(node.fields) do
            local _, value = decode(f.node, r, sub, f.name)
            if type(value) == "string" then r.values[f.name] = value end
        end
        close()
        return sub
    elseif k == "tuple" then
        local sub, close = open(tree, r, label)
        for i, item in ipairs(node.items) do
            local index = "[" .. (i - 1) .. "]"
            local added = decode(item, r, sub, index)
            if added and is_leaf(item) then added:prepend_text(index .. " ") end
        end
        close()
        return sub
    elseif k == "option" then
        local present = r.tvb(r.pos, 1):uint()
        if present > 1 then error("invalid option tag " .. present) end
        local sub, close = open(tree, r, label and (present == 1 and label or label .. ": None"))
        r:bytes(1)
        local _, value
        if present == 1 then _, value = decode(node.of, r, sub, is_leaf(node.of) and label or nil) end
        close()
        return sub, value
    elseif k == "seq" and node.of.k ~= "u8" then
        local sub, close = open(tree, r, label)
        local n = r:varint()
        for i = 1, n do
            local index = "[" .. (i - 1) .. "]"
            local added = decode(node.of, r, sub, index)
            if added and is_leaf(node.of) then added:prepend_text(index .. " ") end
        end
        close()
        return sub
    end
    return decode_leaf(node, r, tree)
end

local function summary(kind, r)
    if SCHEMA[kind].k == "enum" then return table.concat(r.names, " ") end
    return r.values.firmware_version
end

local transfer_type = Field.new("usb.transfer_type")
local URB_INTERRUPT, URB_CONTROL = 1, 2

local function classify(tvb, pinfo)
    local ttype = transfer_type()
    ttype = ttype and ttype() or URB_INTERRUPT
    local from_host = tostring(pinfo.src) == "host"
    local len = tvb:len()
    if ttype == URB_INTERRUPT and len == 64 then
        return from_host and "Command" or "Event", tvb()
    elseif ttype == URB_CONTROL and len == 64 and not from_host then
        return "DeviceInfo", tvb()
    elseif ttype == URB_CONTROL and len == 72 and from_host then
        return "Command", tvb(8)
    end
end

function proto.dissector(tvb, pinfo, tree)
    local kind, body = classify(tvb, pinfo)
    if not kind then return 0 end

    pinfo.cols.protocol = proto.name
    local sub = tree:add(proto, body, "faulty-deck " .. kind)
    local r = reader(body)
    local ok, err = pcall(decode, SCHEMA[kind], r, sub)
    local name
    if not ok then
        sub:add_proto_expert_info(ef_malformed, tostring(err))
        name = "malformed"
    else
        name = summary(kind, r)
        if r.pos < body:len() then sub:add(fields.padding, body(r.pos)) end
    end
    pinfo.cols.info = name and (kind .. ": " .. name) or kind
    return tvb:len()
end

DissectorTable.get("usb.product"):add(0x12090001, proto)

local function heuristic(tvb, pinfo, tree)
    if not proto.prefs.heuristic or tvb:len() ~= 64 then return false end
    return proto.dissector(tvb, pinfo, tree) > 0
end
proto:register_heuristic("usb.interrupt", heuristic)