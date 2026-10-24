// Test-only transport. No networking, environment access, or account operations.
const std = @import("std");
const auth = @import("authentication.zig");
pub const Step = struct { status:u16=200, body:[]const u8="", failure:?[]const u8=null };
pub const Captured = struct {kind:[]const u8,url:[]const u8,payload:[]const u8,max_response_bytes:usize,timeout_ms:u32};
pub var steps:[]const Step=&.{};
pub var index:usize=0;
pub var captured:std.ArrayList(Captured)=.empty;
pub const StdHttpTransport=struct {
 io:std.Io,environ_map:?*const std.process.Environ.Map,user_agent:[]const u8,
 pub fn transport(self:*StdHttpTransport)auth.Transport{return .{.context=self,.request_fn=request};}
 fn request(_: *anyopaque,a:std.mem.Allocator,r:auth.HttpRequest)auth.TransportError!auth.HttpResponse {
  const entry:Captured=switch(r){
   .form_post=>|v| .{.kind="FormPost",.url=v.url,.payload=v.body,.max_response_bytes=v.max_response_bytes,.timeout_ms=v.timeout_ms},
   .json_post=>|v| .{.kind="JsonPost",.url=v.url,.payload=v.body,.max_response_bytes=v.max_response_bytes,.timeout_ms=v.timeout_ms},
   .bearer_get=>|v| .{.kind="BearerGet",.url=v.url,.payload=v.bearer_token,.max_response_bytes=v.max_response_bytes,.timeout_ms=v.timeout_ms},
  };
  // Capture storage must outlive request arenas in the pinned implementation.
  const page=std.heap.page_allocator;
  try captured.append(page,.{.kind=entry.kind,.url=try page.dupe(u8,entry.url),.payload=try page.dupe(u8,entry.payload),.max_response_bytes=entry.max_response_bytes,.timeout_ms=entry.timeout_ms});
  if(index>=steps.len)return error.InvalidResponse;const step=steps[index];index+=1;
  if(step.failure)|failure|{if(std.mem.eql(u8,failure,"Canceled"))return error.Canceled;if(std.mem.eql(u8,failure,"Timeout"))return error.Timeout;if(std.mem.eql(u8,failure,"ResponseTooLarge"))return error.ResponseTooLarge;if(std.mem.eql(u8,failure,"InvalidResponse"))return error.InvalidResponse;return error.NetworkError;}
  return .{.status=@enumFromInt(step.status),.storage=try a.dupe(u8,step.body),.body_len=step.body.len};
 }
};
