#!/usr/bin/env python3
"""Disposable Linux server compatibility matrix against the pinned Zig oracle.

RPC envelopes compare parsed JSON (object ordering is not asserted here). CLI
compares exit status/stdout/stderr byte-for-byte. Only generated backup filename
timestamps are normalized. No real Minecraft or user server is started.
"""
import argparse, hashlib, json, pathlib, subprocess, tempfile
p=argparse.ArgumentParser()
p.add_argument('--zig',required=True)
p.add_argument('--rust',required=True)
p.add_argument('--oracle-source',required=True)
a=p.parse_args()
zig=str(pathlib.Path(a.zig).resolve());rust=str(pathlib.Path(a.rust).resolve())
pinned='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()==pinned, 'wrong oracle commit'
print(json.dumps({'oracle_commit':pinned,'zig_sha256':hashlib.sha256(pathlib.Path(zig).read_bytes()).hexdigest(),'rust_sha256':hashlib.sha256(pathlib.Path(rust).read_bytes()).hexdigest()}))
matrix_results=[]

def matrix_0():
    methods=['server.get','server.list','server.delete','server.properties.get','server.properties.set','server.status','server.logs','server.command','server.stop','server.start','server.restart']
    checks=0;diff=[]
    with tempfile.TemporaryDirectory() as tmp:
     def run(exe,method,params):
      p=subprocess.run([exe,'rpc'],input=json.dumps(dict(protocol=1,id='t',method=method,**({} if params=='MISSING' else {'params':params})))+'\n',text=True,capture_output=True,timeout=10)
      return json.loads(p.stdout)
     for method in methods:
      for params in ['MISSING',None,[],{}, {'directory':'','id':'x'},{'directory':tmp,'id':'../x'},{'directory':tmp,'id':'missing'},{'directory':tmp,'id':'missing','command':''},{'directory':tmp,'id':'missing','properties':{}},{'directory':tmp,'id':'missing','grace_ms':'bad'},{'directory':tmp,'id':'missing','max_bytes':0},{'directory':tmp,'id':'missing','id_extra':None}]:
       a,b=run(zig,method,params),run(rust,method,params);checks+=1
       if a!=b:diff.append((method,params,a,b))
     for method in methods:
      for variant in [{},{'command':'say test'},{'properties':{'motd':'hello'}},{'grace_ms':0},{'max_bytes':5},{'unknown':False}]:
       results=[]
       for exe in [zig,rust]:
        with tempfile.TemporaryDirectory(dir=tmp) as root:
         server=pathlib.Path(root)/'test';server.mkdir();(server/'server.json').write_text(json.dumps(dict(id='test',name='Test',version_id='fake')));(server/'server.properties').write_text('level-name=world\nrcon.password=secret\n')
         results.append(run(exe,method,dict(directory=root,id='test',**variant)))
       checks+=1
       if results[0]!=results[1]:diff.append((method,variant,*results))
    print('checks',checks,'differences',len(diff))
    for d in diff:print(json.dumps(d))
    
    matrix_results.append((checks,len(diff)))

matrix_0()

def matrix_1():
    checks=0;diff=[]
    for command in ['list','show','delete','properties get','properties set','status','logs','command','stop','start','restart']:
     for variant in [[],['bad/id'],['test'],['test','--json'],['test','--unknown'],['test','--servers-directory',''],['test','--grace-ms','-1'],['test','motd=hello']]:
      results=[]
      for exe in [zig,rust]:
       with tempfile.TemporaryDirectory() as root:
        server=pathlib.Path(root)/'test';server.mkdir();(server/'server.json').write_text(json.dumps(dict(id='test',name='Test',version_id='fake')));(server/'server.properties').write_text('level-name=world\nrcon.password=secret\n')
        args=['server']+command.split()+variant+['--servers-directory',root]
        p=subprocess.run([exe]+args,capture_output=True,text=True,timeout=10);results.append((p.returncode,p.stdout,p.stderr))
      checks+=1
      if results[0]!=results[1]:diff.append((command,variant,*results))
    print('checks',checks,'diffs',len(diff))
    for d in diff:print(json.dumps(d))
    
    matrix_results.append((checks,len(diff)))

matrix_1()

def matrix_2():
    methods=['server.worlds.'+s for s in ['list','get','rename','delete','backup','backups','restore','backups.delete']]
    checks=0;diff=[]
    def run(exe,method,params):
     req=dict(protocol=1,id='w',method=method)
     if params!='MISSING':req['params']=params
     p=subprocess.run([exe,'rpc'],input=json.dumps(req)+'\n',text=True,capture_output=True,timeout=10);return json.loads(p.stdout)
    with tempfile.TemporaryDirectory() as tmp:
     for method in methods:
      for params in ['MISSING',None,{},[],{'directory':tmp,'id':'missing'},{'directory':tmp,'id':'missing','world':'world','new_name':'renamed','backup':'x.zip'},{'directory':tmp,'id':'missing','world':'','new_name':'renamed','backup':''}]:
       a,b=run(zig,method,params),run(rust,method,params);checks+=1
       if a!=b:diff.append((method,params,a,b))
     for method in methods:
      for variant in [{},{'world':'world'},{'world':'world','new_name':'renamed'},{'world':'unrelated','new_name':'renamed'},{'world':'bad/name','new_name':'x'},{'backup':'missing.zip'},{'backup':'bad/name'},{'replace':None}]:
       results=[]
       for exe in [zig,rust]:
        with tempfile.TemporaryDirectory(dir=tmp) as root:
         server=pathlib.Path(root)/'test';server.mkdir();(server/'server.json').write_text(json.dumps(dict(id='test',name='Test',version_id='fake')));(server/'server.properties').write_text('level-name=world\nrcon.password=secret\n')
         for name in ['world','world_nether','world_the_end','unrelated']:
          (server/name).mkdir();(server/name/'data').write_text(name)
         response=run(exe,method,dict(directory=root,id='test',**variant))
         if method.endswith('.backup') and response.get('result',{}).get('backup'):response['result']['backup']='<timestamp>.zip'
         results.append(response)
       checks+=1
       if results[0]!=results[1]:diff.append((method,variant,*results))
    print('checks',checks,'diffs',len(diff))
    for d in diff:print(json.dumps(d))
    
    matrix_results.append((checks,len(diff)))

matrix_2()

def matrix_3():
    checks=0;diff=[]
    for command in ['list','show','rename','delete','backup','backups','restore','backups-delete']:
     for variant in [[],['test'],['test','world'],['test','world','new'],['test','--json'],['test','world','--json'],['test','world','new','--json'],['missing'],['test','bad/name'],['test','--unknown'],['bad/id']]:
      results=[]
      for exe in [zig,rust]:
       with tempfile.TemporaryDirectory() as root:
        server=pathlib.Path(root)/'test';server.mkdir();(server/'server.json').write_text(json.dumps(dict(id='test',name='Test',version_id='fake')));(server/'server.properties').write_text('level-name=world\nrcon.password=secret\n')
        for name in ['world','world_nether','world_the_end','unrelated']:
         (server/name).mkdir();(server/name/'data').write_text(name)
        args=['server','worlds',command]+variant+['--servers-directory',root]
        p=subprocess.run([exe]+args,capture_output=True,text=True,timeout=10)
        out=p.stdout
        if command=='backup' and out.startswith('backup:'):out='backup:<timestamp>'
        results.append((p.returncode,out,p.stderr))
      checks+=1
      if results[0]!=results[1]:diff.append((command,variant,*results))
    print('checks',checks,'diffs',len(diff))
    for d in diff:print(json.dumps(d))
    
    matrix_results.append((checks,len(diff)))

matrix_3()

print(json.dumps({"checks":sum(n for n,d in matrix_results),"differences":sum(d for n,d in matrix_results)}))
raise SystemExit(bool(sum(d for n,d in matrix_results)))
