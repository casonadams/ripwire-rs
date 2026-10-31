; ripwire-rs Protobuf tags
(message
  (message_name) @name) @definition.class

(service
  (service_name) @name) @definition.class

(rpc
  (rpc_name) @name) @definition.function

(enum
  (enum_name) @name) @definition.class

; A message or enum named as a field type, map value, oneof member, or rpc request/response is a use
; of that definition: `repeated Item items = 2;` and `rpc Create(CreateReq) returns (Order);` each
; reference their message.
(message_or_enum_type
  (identifier) @name) @reference.type
