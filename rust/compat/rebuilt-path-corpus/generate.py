import json,pathlib,copy,sys
p=pathlib.Path(sys.argv[1])
p.mkdir(parents=True,exist_ok=True)
cases=[]
def f(path):return dict(path=path,hashes={'sha1':'1'*40},downloads=['https://example.test/a'],fileSize=1)
def mr(name,files,expect='ok'):cases.append((name,'mr',dict(formatVersion=1,game='minecraft',name='fixture',dependencies={'minecraft':'1.21'},files=files),expect))
def cf(name,path,expect='ok',**kw):
 d=dict(id=1,modId=1,fileName=path,releaseType=1,hashes=[{'value':'1'*40,'algo':1}],fileDate='',fileLength=1,downloadUrl='https://example.test/a',gameVersions=['1.21']);d.update(kw);cases.append((name,'cf',{'data':d},expect))
mr('mr_opaque',[f([109,111,100,115,47,255])]);mr('mr_nul',[f([255,0])],'UnsafePath')
mr('mr_512',[f([255]*512)]);mr('mr_513',[f([255]*513)],'InvalidModpackFile')
a=f([109,111,100,115,47,255]);a['env']={'client':'unsupported'}
mr('mr_skip',[a]);b=copy.deepcopy(a);b['hashes']['sha1']='';mr('mr_duplicate_before_later_hash',[a,b],'DuplicateModpackFile')
a=copy.deepcopy(a);a['hashes']['sha1']='';mr('mr_skipped_bad_hash',[a],'InvalidModrinthHash')
a=f([0]);a['fileSize']=0;mr('mr_path_before_size',[a],'UnsafePath')
mr('mr_numeric',[f(['109',111.0,100,115,47,255])]);mr('mr_overflow',[f([256])],'Overflow');mr('mr_fraction',[f([65.5])],'InvalidNumber');mr('mr_null',[f(None)],'UnexpectedToken')
cf('cf_opaque',[255]);cf('cf_nul',[0],'InvalidCurseforgeFilename');cf('cf_256',[255]*256);cf('cf_257',[255]*257,'InvalidCurseforgeFilename');cf('cf_name_before_release',[0],'InvalidCurseforgeFilename',releaseType=0);cf('cf_id_before_name',[0],'InvalidCurseforgeId',id=0);cf('cf_empty',[],'InvalidCurseforgeFilename');cf('cf_numeric',['255']);cf('cf_overflow',[256],'Overflow')
lines=['const std = @import("std");','const core = @import("core");','test "rebuilt_path_oracle_pure_schema_20261106" {']
for name,kind,value,expect in cases:
 (p/(name+'.json')).write_text(json.dumps(value,separators=(',',':')))
 fun='core.mrpack.parse' if kind=='mr' else 'core.curseforge.parseFile'
 lines+=['    {',f'        const result = {fun}(std.testing.allocator, @embedFile("{name}.json"));', '        if (result) |parsed_value| {','            var parsed = parsed_value; defer parsed.deinit();',f'            std.debug.print("{name}\\tok\\n", .{{}});',f'            try std.testing.expectEqualStrings("{expect}", "ok");']
 if kind=='mr' and name=='mr_skip':lines+=['            try std.testing.expect(!core.mrpack.shipsToClient(parsed.value.files[0]));']
 if kind=='mr' and name=='mr_opaque':lines+=['            try std.testing.expectEqual(@as(u8, 255), parsed.value.files[0].path[5]);']
 if kind=='cf' and name=='cf_opaque':lines+=['            try std.testing.expectEqual(@as(u8, 255), parsed.value.data.fileName[0]);']
 lines+=['        } else |err| {',f'            std.debug.print("{name}\\t{{s}}\\n", .{{@errorName(err)}});',f'            try std.testing.expectEqualStrings("{expect}", @errorName(err));','        }','    }']
lines+=['}'];(p/'oracle.zig').write_text('\n'.join(lines)+'\n');(p/'cases.json').write_text(json.dumps([{'name':a,'kind':b,'expected':d} for a,b,c,d in cases],indent=2))
