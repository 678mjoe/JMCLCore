#!/usr/bin/env python3
"""Pinned RPC/CLI remote compatibility with disposable, loopback-only fixtures.
No Minecraft servers or credentials: a benign owned sleeper supplies PID identity.
"""
import argparse, hashlib, json, pathlib, socket, struct, subprocess, sys, tempfile, threading, time
p=argparse.ArgumentParser();p.add_argument('--zig',required=True);p.add_argument('--rust',required=True);p.add_argument('--oracle-source',required=True);a=p.parse_args()
zig=str(pathlib.Path(a.zig).resolve());rust=str(pathlib.Path(a.rust).resolve());pin='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()==pin
print(json.dumps({'oracle_commit':pin,'zig_sha256':hashlib.sha256(pathlib.Path(zig).read_bytes()).hexdigest(),'rust_sha256':hashlib.sha256(pathlib.Path(rust).read_bytes()).hexdigest()}))
count=0;differences=[]
def check(label,left,right):
 global count
 count+=1
 if left!=right:differences.append((label,left,right))
def rpc(exe,method,params):
 request={'protocol':1,'id':'remote','method':method}
 if params!='MISSING':request['params']=params
 out=subprocess.run([exe,'rpc'],input=json.dumps(request)+'\n',text=True,capture_output=True,timeout=12)
 assert out.returncode==0,out.stderr
 return json.loads(out.stdout)
def cli(exe,args):
 out=subprocess.run([exe,'server']+args,text=True,capture_output=True,timeout=12)
 return out.returncode,out.stdout,out.stderr
with tempfile.TemporaryDirectory() as temp:
 root=pathlib.Path(temp);server=root/'fixture';server.mkdir();(server/'.jmcl').mkdir()
 (server/'server.json').write_text(json.dumps({'id':'fixture','name':'Fixture','version_id':'synthetic'}))
 (server/'server.properties').write_text('enable-rcon=false\nenable-query=false\n')
 base={'directory':temp,'id':'fixture'}
 for method in ['server.rcon.command','server.query']:
  for value in ['MISSING',None,[],{},base,dict(base,password='synthetic-only',command='list'),dict(base,password='',command='list'),dict(base,password='synthetic-only',command='list',max_bytes=0),dict(base,password='synthetic-only',command='list',max_bytes='2e1'),dict(base,mode='bad'),dict(base,id='../invalid',password='synthetic-only',command='list'),dict(base,id='absent',password='synthetic-only',command='list')]:check((method,value),rpc(zig,method,value),rpc(rust,method,value))
 password=root/'password';password.write_text('synthetic-only\r\n')
 for args in [[],['query'],['query','bad/id'],['query','fixture'],['query','fixture','--json'],['query','fixture','--mode','bad'],['rcon'],['rcon','command'],['rcon','command','fixture'],['rcon','command','fixture','--command','list','--password-file',str(password)],['rcon','command','fixture','--command','list','--password-file',str(password),'--max-bytes','0']]:
  args=args+['--servers-directory',temp];check(args,cli(zig,args),cli(rust,args))
 identity='0123456789abcdef0123456789abcdef'
 sleeper=subprocess.Popen([sys.executable,'-c','import time; time.sleep(120)','-Djmcl.server.identity='+identity],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 try:
  deadline=time.monotonic()+2
  while identity.encode() not in pathlib.Path(f'/proc/{sleeper.pid}/cmdline').read_bytes():
   assert time.monotonic()<deadline;time.sleep(.01)
  (server/'.jmcl/server.pid').write_text(json.dumps({'pid':sleeper.pid,'supervisor_pid':__import__('os').getpid(),'identity':identity,'java_path':'synthetic','started_at_ms':1}))
  for method in ['server.rcon.command','server.query']:
   check('disabled '+method,rpc(zig,method,dict(base,password='synthetic-only',command='list')),rpc(rust,method,dict(base,password='synthetic-only',command='list')))
  def exact(stream,n):
   data=b''
   while len(data)<n:
    chunk=stream.recv(n-len(data));assert chunk;data+=chunk
   return data
  def frame(stream):
   size=struct.unpack('<i',exact(stream,4))[0];assert 10<=size<=65536
   data=exact(stream,size);return struct.unpack('<ii',data[:8])+(data[8:-2],)
  def send(stream,id,kind,body):stream.sendall(struct.pack('<iii',len(body)+10,id,kind)+body+b'\0\0')
  def rcon_fixture(exe,case):
   listener=socket.socket();listener.bind(('127.0.0.1',0));listener.listen();listener.settimeout(4)
   (server/'server.properties').write_text(f'enable-rcon=true\nrcon.port={listener.getsockname()[1]}\nserver-ip=never-dial.invalid\n')
   failures=[]
   def serve():
    try:
     stream,_=listener.accept();stream.settimeout(4)
     with stream:
      auth,kind,password=frame(stream);assert kind==3 and password==b'synthetic-only'
      if case=='auth-failure':send(stream,-1,2,b'');return
      send(stream,auth,0,b'');send(stream,auth,2,b'')
      command,kind,text=frame(stream);assert kind==2 and text==b'list'
      if case=='wrong-id':send(stream,command+10,0,b'bad');return
      send(stream,command,0,'你好'.encode())
      sentinel,kind,body=frame(stream);assert kind==2 and not body and sentinel!=command
      if case=='no-sentinel':return
      send(stream,command,0,b' world');send(stream,sentinel,0,b'')
    except Exception as e:failures.append(repr(e))
    finally:listener.close()
   thread=threading.Thread(target=serve);thread.start()
   try:result=rpc(exe,'server.rcon.command',dict(base,password='synthetic-only',command='list',max_bytes=4 if case=='truncate' else 1024))
   finally:thread.join(5)
   assert not thread.is_alive() and not failures,failures
   return result
  for case in ['normal','truncate','auth-failure','wrong-id','no-sentinel']:check('rcon '+case,rcon_fixture(zig,case),rcon_fixture(rust,case))
  def query_fixture(exe,case):
   udp=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);udp.bind(('127.0.0.1',0));udp.settimeout(4)
   (server/'server.properties').write_text(f'enable-query=true\nquery.port={udp.getsockname()[1]}\nserver-ip=never-dial.invalid\n')
   failures=[]
   def serve():
    try:
     data,peer=udp.recvfrom(64);assert len(data)==7;session=data[3:7]
     udp.sendto(b'\x09'+session+b'-42\0',peer);data,peer=udp.recvfrom(64);assert struct.unpack('>i',data[7:11])[0]==-42
     if case=='wrong-session':session=bytes([session[0]^1])+session[1:]
     if case=='basic':body=b'fixture\0SMP\0world\x001\x0020\0\xdd\x63127.0.0.1\0'
     else:body=b'splitnum\0\x80\0hostname\0fixture\0gametype\0SMP\0map\0world\0version\x001.21\0game_id\0MINECRAFT\0numplayers\x001\0maxplayers\x0020\0hostport\x0025565\0hostip\x00127.0.0.1\0\0\x01player_\0\0Alex\0\0'
     udp.sendto(b'\0'+session+body,peer)
    except Exception as e:failures.append(repr(e))
    finally:udp.close()
   thread=threading.Thread(target=serve);thread.start()
   try:result=rpc(exe,'server.query',dict(base,mode='basic' if case=='basic' else 'full'))
   finally:thread.join(5)
   assert not thread.is_alive() and not failures,failures
   return result
  for case in ['basic','full','wrong-session']:check('query '+case,query_fixture(zig,case),query_fixture(rust,case))
 finally:
  sleeper.terminate();sleeper.wait(timeout=5)
print(json.dumps({'checks':count,'differences':len(differences)}))
for difference in differences:print(json.dumps(difference))
raise SystemExit(bool(differences))
