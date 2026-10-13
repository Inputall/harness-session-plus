// Read-only live CDP structural inspection. No title, body, ID, resource, or attribute values exported.
const expression = `(() => {
 if (location.href !== 'dsh-app://app/') throw new Error('Page mismatch');
 return [...document.querySelectorAll('[role="menu"], [role="menuitem"]')].slice(0,30).map(el => ({
   tag: el.tagName, role: el.getAttribute('role'),
   archiveExact: el.textContent?.trim() === '归档会话',
   archiveLabel: [...el.querySelectorAll('span')].some(s => s.textContent?.trim() === '归档会话'),
   parentTag: el.parentElement?.tagName, parentRole: el.parentElement?.getAttribute('role'),
   children: [...el.children].map(child => ({tag: child.tagName, archiveExact: child.textContent?.trim() === '归档会话'}))
 }));
})()`;
(async () => {
 const response = await fetch('http://127.0.0.1:9223/json/list',{redirect:'error',signal:AbortSignal.timeout(5000)});
 const targets = (await response.json()).filter(t=>t.type==='page'&&t.url==='dsh-app://app/');
 if(targets.length!==1)throw new Error('Expected exactly one Harness target');
 const url = new URL(targets[0].webSocketDebuggerUrl);
 if(url.protocol!=='ws:'||url.hostname!=='127.0.0.1'||url.port!=='9223'||url.username||url.password)throw new Error('Invalid endpoint');
 const socket=new WebSocket(url);
 const timer=setTimeout(()=>{socket.close();process.exitCode=1;},5000);
 socket.onopen=()=>socket.send(JSON.stringify({id:1,method:'Runtime.evaluate',params:{expression,returnByValue:true}}));
 socket.onmessage=event=>{const result=JSON.parse(event.data);if(result.id!==1)return;clearTimeout(timer);if(result.error||result.result?.exceptionDetails){console.error('Inspection failed');process.exitCode=1;}else console.log(JSON.stringify(result.result?.result?.value,null,2));socket.close();};
 socket.onerror=()=>{clearTimeout(timer);console.error('CDP connection failed');process.exitCode=1;};
})().catch(()=>{console.error('Unable to inspect loopback Harness page');process.exitCode=1;});
