const std=@import("std");const core=@import("jmcl_core");
const MicrosoftDevicePollParams = struct {
    client_id: []const u8,
    device_code: []const u8,
};
const MicrosoftRefreshParams = struct {
    client_id: []const u8,
    refresh_token: []const u8,
};
const missing=[_] []const u8{"auth.microsoft.device.begin requires a Microsoft client ID","auth.microsoft.device.poll requires a client ID and device code","auth.microsoft.refresh requires a client ID and refresh token","auth.minecraft.exchange requires a client ID and Microsoft access token","auth.minecraft.refresh_exchange requires a client ID and refresh token"};
const invalid=[_] []const u8{"Microsoft device authorization parameters are invalid","Microsoft device polling parameters are invalid","Microsoft refresh parameters are invalid","Minecraft exchange parameters are invalid","Minecraft refresh-exchange parameters are invalid"};
fn failure(j:*std.json.Stringify,phase:[]const u8,message:[]const u8)!void{_ = phase;try j.write(.{.code="INVALID_PARAMS",.message=message});}
pub fn main(init:std.process.Init)!void{
 const a=init.arena.allocator();const args=try init.minimal.args.toSlice(a);const mode=try std.fmt.parseInt(u8,args[1],10);
 var buf:[4096]u8=undefined;var w:std.Io.File.Writer=.init(.stdout(),init.io,&buf);var j:std.json.Stringify=.{.writer=&w.interface};
 if(std.mem.eql(u8,args[3],"absent")){try failure(&j,"prepare",missing[mode]);}else{
 const b=try std.Io.Dir.cwd().readFileAlloc(init.io,args[2],a,.limited(65536));const v=try std.json.parseFromSlice(std.json.Value,a,b,.{});defer v.deinit();
 switch(mode){0=>try output(core.authentication.Config,a,v.value,&j,mode),1=>try output(MicrosoftDevicePollParams,a,v.value,&j,mode),3=>try output(core.authentication.MicrosoftAccess,a,v.value,&j,mode),2,4=>try output(MicrosoftRefreshParams,a,v.value,&j,mode),else=>unreachable,}
 }
 try w.interface.writeByte('\n');try w.interface.flush();
}
fn output(comptime T:type,a:std.mem.Allocator,value:std.json.Value,j:*std.json.Stringify,mode:u8)!void{
 const parsed=std.json.parseFromValue(T,a,value,.{.ignore_unknown_fields=true}) catch {return failure(j,"prepare",invalid[mode]);};defer parsed.deinit();const input=parsed.value;
 // Source-derived protocol empty checks/messages, separately verified against
 // pinned protocol source. No handler is invoked by this fixture.
 if(comptime @hasField(T,"device_code")){
  if(input.client_id.len==0 or input.device_code.len==0)return failure(j,"prepare","Microsoft client ID and device code must not be empty");
 }else if(comptime @hasField(T,"access_token")){
  if(input.client_id.len==0 or input.access_token.len==0)return failure(j,"prepare","Microsoft client ID and access token must not be empty");
 }else if(comptime @hasField(T,"refresh_token")){
  if(input.client_id.len==0 or input.refresh_token.len==0)return failure(j,"prepare","Microsoft client ID and refresh token must not be empty");
 }
 const secondary:?[]const u8=if(comptime @hasField(T,"locale"))input.locale else if(comptime @hasField(T,"device_code"))input.device_code else if(comptime @hasField(T,"access_token"))input.access_token else input.refresh_token;
 core.authentication.__fixtureValidateRequest(mode,input.client_id,secondary) catch |e| switch(e){
  error.InvalidClientId,error.InvalidLocale,error.InvalidDeviceCode,error.InvalidRefreshToken,error.InvalidAccessToken=>return failure(j,"semantic","Authentication parameters are invalid"),else=>return e,
 };
 try j.write(.{.allowed_by_pure_guards=true});
}
