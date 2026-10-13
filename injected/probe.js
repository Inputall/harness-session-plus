(() => {
  // Structural evidence only: no text, values, IDs, URLs, inline source, storage, or application methods.
  const counts = {};
  document.querySelectorAll('[role]').forEach(el => {
    const role = el.getAttribute('role');
    if (['menu','menuitem','button','dialog','navigation','list','listitem','tree','treeitem'].includes(role)) counts[role] = (counts[role] || 0) + 1;
  });
  const attributes = new Set();
  document.querySelectorAll('[role="menu"],[role="menuitem"]').forEach(el => {
    for (const name of el.getAttributeNames()) if (/^(data-|aria-)/.test(name)) attributes.add(name);
  });
  return {
    schemaVersion: 1,
    pageMatches: location.href === 'dsh-app://app/',
    roleCounts: counts,
    menuAttributeNames: [...attributes].sort().slice(0,100),
    scriptCounts: {external: document.querySelectorAll('script[src]').length, inline: document.querySelectorAll('script:not([src])').length},
    customElementCount: new Set([...document.querySelectorAll('*')].map(el => el.localName).filter(name => name.includes('-'))).size,
    privacy: 'No chat text, identifiers, attribute values, resource URLs, authentication, storage, or inline source collected'
  };
})()
