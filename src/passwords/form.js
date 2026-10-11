(() => {
  if (window.top !== window || window.__poeziPasswords) return;
  window.__poeziPasswords = true;
  const post = (message) => {
    try { window.webkit.messageHandlers.poeziPw.postMessage(JSON.stringify(message)); } catch (e) {}
  };
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    const s = getComputedStyle(el);
    return r.width > 2 && r.height > 2 && s.visibility !== 'hidden' && s.display !== 'none' && Number(s.opacity) > 0.05;
  };
  const passwordFields = () => Array.from(document.querySelectorAll('input[type=password]'))
    .filter((f) => !f.disabled && !f.readOnly && visible(f));
  const textTypes = ['text', 'email', 'tel', ''];
  const usernameFor = (pw) => {
    const scope = pw.form || document;
    const inputs = Array.from(scope.querySelectorAll('input'))
      .filter((i) => textTypes.includes((i.getAttribute('type') || '').toLowerCase()) && !i.disabled && visible(i));
    const before = inputs.filter((i) => i.compareDocumentPosition(pw) & Node.DOCUMENT_POSITION_FOLLOWING);
    return before.length ? before[before.length - 1] : null;
  };
  const isSignup = (fields) => fields.length >= 2 || fields.some((f) => (f.getAttribute('autocomplete') || '').includes('new-password'));
  let last = '';
  const report = () => {
    const fields = passwordFields();
    const state = JSON.stringify({ type: 'fields', login: fields.length > 0, signup: isSignup(fields) });
    if (state !== last) {
      last = state;
      post(JSON.parse(state));
    }
  };
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  const setValue = (el, value) => {
    el.focus();
    setter.call(el, value);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  window.__poeziFill = (username, password) => {
    const fields = passwordFields();
    if (!fields.length) return false;
    const pw = fields[0];
    const user = usernameFor(pw);
    if (user && username) setValue(user, username);
    setValue(pw, password);
    return true;
  };
  window.__poeziFillNew = (password) => {
    const fields = passwordFields();
    const targets = isSignup(fields) && fields.length >= 2 ? fields : fields.slice(-1);
    targets.forEach((f) => setValue(f, password));
    return targets.length > 0;
  };
  const capture = (form) => {
    const fields = passwordFields().filter((f) => !form || f.form === form || !f.form);
    const pw = fields.find((f) => f.value);
    if (!pw) return;
    const user = usernameFor(pw);
    post({ type: 'submit', username: user ? user.value : '', password: pw.value, signup: isSignup(fields) });
  };
  document.addEventListener('submit', (e) => { if (e.isTrusted) capture(e.target); }, true);
  document.addEventListener('click', (e) => {
    if (!e.isTrusted || !e.target || !e.target.closest) return;
    const button = e.target.closest('button, input[type=submit], [role=button]');
    if (button && passwordFields().some((f) => f.value)) capture(button.form || null);
  }, true);
  document.addEventListener('keydown', (e) => {
    if (e.isTrusted && e.key === 'Enter' && e.target && e.target.type === 'password') capture(e.target.form || null);
  }, true);
  const start = () => {
    report();
    let timer = 0;
    new MutationObserver(() => {
      clearTimeout(timer);
      timer = setTimeout(report, 400);
    }).observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['type', 'style', 'class', 'hidden'] });
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start); else start();
})();
