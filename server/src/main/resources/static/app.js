const authScreen = document.querySelector('#auth-screen');
const appShell = document.querySelector('#app-shell');
const authForm = document.querySelector('#auth-form');
const authSubmit = document.querySelector('#auth-submit');
const authError = document.querySelector('#auth-error');
const authSwitch = document.querySelector('#auth-switch');
const authTitle = document.querySelector('#auth-title');
const authPrompt = document.querySelector('#auth-prompt');
const nameField = document.querySelector('#name-field');
const messages = document.querySelector('#messages');
const form = document.querySelector('#message-form');
const input = document.querySelector('#message-input');
const profileName = document.querySelector('#profile-name');
const profileEmail = document.querySelector('#profile-email');
const logoutButton = document.querySelector('#logout-button');
let signupMode = true;
let currentUser;
let pollTimer;

function escapeHtml(value) {
  const element = document.createElement('span');
  element.textContent = value;
  return element.innerHTML;
}

function renderMessage(message) {
  const row = document.createElement('article');
  const name = (message?.name ?? 'Unknown').trim() || 'Unknown';
  const text = (message?.text ?? '').trim();
  row.className = `message-row${message?.mine ? ' mine' : ''}`;
  const avatarClass = message?.mine ? 'avatar-you' : 'avatar-maya';
  row.innerHTML = `<div class="avatar ${avatarClass}">${escapeHtml(name[0].toUpperCase())}</div><div class="message"><div class="message-meta"><strong>${escapeHtml(name)}</strong><time>${escapeHtml(message?.time ?? 'now')}</time></div><p class="message-text">${escapeHtml(text)}</p></div>`;
  messages.append(row);
}

async function api(path, options = {}) {
  let response;
  try {
    response = await fetch(path, { ...options, headers: { 'Content-Type': 'application/json', ...(options.headers || {}) } });
  } catch (error) {
    throw new Error('Unable to connect to Gather. Check your internet connection and try again.');
  }
  let body = null;
  try { body = await response.json(); } catch (_) {}
  if (!response.ok) {
    throw new Error(body?.message || body?.detail || body?.error || `Request failed (${response.status})`);
  }
  return body;
}

async function loadMessages(scroll = false) {
  const data = await api('/api/messages');
  messages.replaceChildren();
  data.forEach(renderMessage);
  if (scroll) messages.scrollTop = messages.scrollHeight;
}

function showApp(user) {
  currentUser = user;
  profileName.textContent = user.name;
  profileEmail.textContent = user.email;
  authScreen.hidden = true;
  appShell.hidden = false;
  loadMessages(true).catch((error) => {
    showAuth();
    authError.textContent = error.message;
  });
  clearInterval(pollTimer);
  pollTimer = setInterval(() => loadMessages(false).catch(() => {}), 2000);
}

function showAuth() {
  clearInterval(pollTimer);
  appShell.hidden = true;
  authScreen.hidden = false;
}

function updateAuthMode() {
  signupMode = !signupMode;
  authTitle.textContent = signupMode ? 'Create your account' : 'Welcome back';
  authSubmit.textContent = signupMode ? 'Sign up' : 'Log in';
  authPrompt.textContent = signupMode ? 'Already have an account?' : 'Need an account?';
  authSwitch.textContent = signupMode ? 'Log in' : 'Sign up';
  nameField.hidden = !signupMode;
  authError.textContent = '';
}

authSwitch.addEventListener('click', updateAuthMode);
authForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  authError.textContent = '';
  const formData = new FormData(authForm);
  const payload = signupMode
    ? { name: formData.get('name'), email: formData.get('email'), password: formData.get('password') }
    : { email: formData.get('email'), password: formData.get('password') };
  try {
    showApp(await api(signupMode ? '/api/auth/signup' : '/api/auth/login', { method: 'POST', body: JSON.stringify(payload) }));
    authForm.reset();
  } catch (error) {
    authError.textContent = error.message;
  }
});

logoutButton.addEventListener('click', async () => {
  await api('/api/auth/logout', { method: 'POST' }).catch(() => {});
  showAuth();
});

form.addEventListener('submit', async (event) => {
  event.preventDefault();
  const text = input.value.trim();
  if (!text) return;
  input.value = '';
  try {
    await api('/api/messages', { method: 'POST', body: JSON.stringify({ text }) });
    await loadMessages(true);
  } catch (error) {
    input.value = text;
  }
});

input.addEventListener('keydown', (event) => {
  if (event.key === 'Enter' && !event.shiftKey) form.requestSubmit();
});

api('/api/auth/me').then(showApp).catch(showAuth);
