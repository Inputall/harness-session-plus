// Version-bound read-only queries. Navigation uses the shipped WorkspaceBrowser callback.
(async (adapter, request) => {
  const before = await adapter.snapshot();
  const origin = before.hostOrigin;
  const rpc = (name, args) => adapter.rpc(name, args, origin);
  const archived = new Set(before.workspaces.archivedSessionIds);
  // Read the committed React tree, never invoke hooks, mutate props, or take over event streams.
  let browser = null;
  const statuses = new Map();
  for (const row of document.querySelectorAll('[data-row-key]')) {
    const key = Object.keys(row).find(k => k.startsWith('__reactFiber$'));
    for (let f = key && row[key], depth = 0; f && depth++ < 70; f = f.return) {
      const props = f.memoizedProps;
      if (props && typeof props.open === 'function' && typeof props.searchSessions === 'function'
          && typeof props.useSessions === 'function' && typeof props.useWorkspaces === 'function') browser = props;
      if (props && typeof props.useSessionStatus === 'function') {
        for (let hook = f.memoizedState, count = 0; hook && count++ < 80; hook = hook.next) {
          for (const value of [hook.memoizedState, hook.memoizedState?.value, hook.memoizedState?.[0]]) {
            if (!(value instanceof Map) || !value.size || ![...value.values()].every(v => v &&
                typeof v.running === 'boolean' && typeof v.completionUnread === 'boolean')) continue;
            for (const [id, status] of value) statuses.set(id, {
              pending: ['approval','question','plan-review'].includes(status.pendingInteraction?.kind) ? status.pendingInteraction.kind : null,
              completed: status.completionUnread
            });
          }
        }
      }
      const node = props?.node;
      if (node && typeof node.id === 'string' && typeof node.running === 'boolean'
          && typeof node.completed === 'boolean') statuses.set(node.id, {
        pending: ['approval','question','plan-review'].includes(node.pendingInteraction) ? node.pendingInteraction : null,
        completed: node.completed
      });
    }
  }
  const sessions = before.sessions.map(s => {
    const owners = before.workspaces.items.filter(w => w.sessionIds.includes(s.sessionId));
    return {...s, archived: archived.has(s.sessionId), workspace: owners.length === 1 ? owners[0].title : '',
      statusObserved: statuses.has(s.sessionId), ...statuses.get(s.sessionId)};
  });
  const history = async sessionId => {
    const projections = await rpc('session/projections', {request: {sessionId}});
    if (projections?.kind !== 'sequenced' || !Number.isSafeInteger(projections?.asOfSeq) || projections.asOfSeq < -1) {
      return {projectionState: projections?.kind === 'migration-required' ? 'migration-required' : 'unavailable', values: null, page: {records: [], hasMore: true}};
    }
    const page = await readPage(sessionId, projections.asOfSeq);
    return {page, throughSeq: projections.asOfSeq, values: projections.values || {}, projectionState: 'sequenced'};
  };
  const readPage = async (sessionId, throughSeq) => {
    const page = await rpc('session/page', {request: {
      address: {kind: 'session', sessionId}, ...(Number.isSafeInteger(throughSeq) ? {throughSeq} : {}), maxMessages: 20
    }});
    if (!Array.isArray(page.records)) throw Error('Unsupported history page');
    return page;
  };
  const texts = page => page.records.filter(r => r.type === 'event' &&
    ['user/message','assistant/message'].includes(r.event?.type) && r.event.surfaceOp === 'append').map(r => {
      const event = r.event;
      const message = event.type === 'user/message' ? event.data : event.data?.message;
      return {seq: event.seq, role: event.type === 'user/message' ? 'user' : 'assistant',
        text: (message?.content || []).filter(p => p.type === 'text' && typeof p.text === 'string').map(p => p.text).join('\n')};
    }).filter(m => m.text).slice(-20);
  if (request.action === 'list') return {sessions, hostOrigin: origin, navigationAvailable: !!browser};
  if (request.action === 'search') {
    try {
      const result = await rpc('session/search', {request: {query: request.query}});
      if (!Array.isArray(result.items) || typeof result.hasMore !== 'boolean') throw Error('Unsupported search result');
      return {sessions, hits: result.items, hasMore: result.hasMore, searchMode:'native', hostOrigin: origin};
    } catch (error) {
      if (!error.searchDisabled) throw error;
    }
    // The installed desktop disables its search index. Never enable or write the native index.
    // Bound the fallback by session count, page size, concurrency, and an overall deadline.
    const candidates = sessions.filter(s => s.origin !== 'subagent' && !s.blank).slice(0,50);
    const hits = []; let scanned=0, incomplete=0, failed=0;
    const deadline=Date.now()+4000, needle=request.query.toLocaleLowerCase();
    for (let i=0;i<candidates.length&&Date.now()<deadline;i+=3) {
      await Promise.all(candidates.slice(i,i+3).map(async s=>{
        try {
          const {page}=await history(s.sessionId);scanned++;if(page.hasMore)incomplete++;
          const match=texts(page).find(m=>m.text.toLocaleLowerCase().includes(needle));
          if(match){const at=match.text.toLocaleLowerCase().indexOf(needle);hits.push({sessionId:s.sessionId,snippet:match.text.slice(Math.max(0,at-60),at+180)});}
        }catch{failed++;}
      }));
    }
    return {sessions,hits,hasMore:incomplete>0||scanned<candidates.length||candidates.length<sessions.filter(s=>s.origin!=='subagent'&&!s.blank).length,
      searchMode:'recent',scanned,incomplete,failed,hostOrigin:origin};
  }
  const session = sessions.find(s => s.sessionId === request.sessionId);
  if (!session) throw Error('Session no longer exists');
  if (request.action === 'open') {
    if (session.archived) throw Error('Restore archived session before opening');
    if (!browser) throw Error('Harness sidebar navigation unavailable; expand a workspace and retry');
    if ((await window.dshDesktopBoot.ready()).streamBaseUrl !== origin) throw Error('Host generation changed');
    browser.open(session.sessionId);
    return {opened: true};
  }
  if (request.action !== 'detail') throw Error('Unsupported read action');
  const {page,throughSeq,values,projectionState}=await history(session.sessionId);
  const messages=texts(page).map(m=>({...m,text:m.text.slice(0,6000)}));
  const end = page.records.filter(r => r.type === 'event' && r.event?.type === 'turn/end').at(-1)?.event;
  const pick = (value, keys) => value && typeof value === 'object' ? Object.fromEntries(keys.filter(k => Number.isSafeInteger(value[k]) && value[k] >= 0).map(k => [k, value[k]])) : {};
  const selection = values?.modelSelection;
  const route = value => value && typeof value.provider === 'string' && value.provider && typeof value.model === 'string' && value.model
    ? {provider: value.provider, model: value.model, ...(typeof value.reasoningEffort === 'string' ? {reasoningEffort: value.reasoningEffort} : {})} : null;
  const stats = projectionState === 'sequenced' ? {
    tokenUsage: pick(values?.tokenUsage, ['uncachedInputTokens','outputTokens','cacheReadTokens','cacheWriteTokens']),
    contextPressure: pick(values?.contextPressure, ['pressureTokens','projectedTokens','contextWindow']),
    modelSelection: {lastUsed: route(selection?.lastUsed), next: route(selection?.next)}
  } : null;
  return {session, messages, hasMore: page.hasMore, throughSeq, projectionState, stats,
    lastTurn: end ? {seq: end.seq, reason: end.data?.reason?.kind || 'unknown'} : null};
})
