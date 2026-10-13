// Read installation resources without altering the archive. Only public application code.
const fs = require('node:fs');
const path = require('node:path');
const archive = process.argv[2];
const fd=fs.openSync(archive,'r');
const prefix=Buffer.alloc(16);fs.readSync(fd,prefix,0,16,0);
const headerSize=prefix.readUInt32LE(4),jsonSize=prefix.readUInt32LE(12);
const header=Buffer.alloc(jsonSize);fs.readSync(fd,header,0,jsonSize,16);
const tree=JSON.parse(header.toString());
const entries=[];
function walk(node,base=''){for(const [name,item] of Object.entries(node.files||{})){const key=base+name;if(item.files)walk(item,key+'/');else entries.push({path:key,...item});}}
walk(tree);
if(process.argv[3]){
 const entry=entries.find(e=>e.path===process.argv[3]);if(!entry||entry.unpacked||entry.link)throw new Error('Not a packed resource');
 const bytes=Buffer.alloc(entry.size);fs.readSync(fd,bytes,0,entry.size,8+headerSize+Number(entry.offset));
 if(process.argv[4]){const output=path.resolve(process.argv[4]);if(!output.startsWith(path.resolve('target')+path.sep))throw new Error('Output must be in workspace target');fs.mkdirSync(path.dirname(output),{recursive:true});fs.writeFileSync(output,bytes);}else console.log(bytes.toString());
}else console.log(entries.filter(e=>/package.json$|\.js$|\.map$/.test(e.path)&&(!/node_modules/.test(e.path)||/dsh-(workspace|session-controller|workspace-controller|ui-workspace|connection|session-query)/.test(e.path))).map(e=>({path:e.path,size:e.size})).slice(0,250));
fs.closeSync(fd);
