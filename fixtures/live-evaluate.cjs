// Development tool only, no service. Evaluate a reviewed local expression on the unique Harness page.
const fs = require('node:fs');
(async()=>{
 const targets=(await (await fetch('http://127.0.0.1:9223/json/list',{redirect:'error'})).json()).filter(t=>t.type==='page'&&t.url==='dsh-app://app/');
 if(targets.length!==1)throw Error('Ambiguous page');
 const url=new URL(targets[0].webSocketDebuggerUrl);
 if(url.hostname!=='127.0.0.1'||url.port!=='9223'||url.protocol!=='ws:')throw Error('Invalid endpoint');
 const expression=fs.readFileSync(process.argv[2],'utf8');
 const socket=new WebSocket(url);const timer=setTimeout(()=>{socket.close();process.exitCode=1;},20000);
 socket.onopen=()=>socket.send(JSON.stringify({id:1,method:'Runtime.evaluate',params:{expression:`(()=>{if(location.href!=='dsh-app://app/')throw Error('Wrong page');return (${expression});})()`,returnByValue:true,awaitPromise:true}}));
 socket.onmessage=e=>{const v=JSON.parse(e.data);if(v.id!==1)return;clearTimeout(timer);console.log(JSON.stringify(v.result?.exceptionDetails?{error:'Evaluation failed',description:v.result.result?.description}:v.result?.result?.value??v.error,null,2));socket.close();};
})().catch(e=>{console.error(e.message);process.exitCode=1;});
