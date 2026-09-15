const authScreen = document.querySelector('#auth-screen');
const appShell = document.querySelector('#app-shell');
const authForm = document.querySelector('#auth-form');
const authError = document.querySelector('#auth-error');
const authSwitch = document.querySelector('#auth-switch');
const authTitle = document.querySelector('#auth-title');
const authPrompt = document.querySelector('#auth-prompt');
const accountIdInput = document.querySelector('#account-id-input');
const nameField = document.querySelector('#name-field');
const messages = document.querySelector('#messages');
const form = document.querySelector('#message-form');
const input = document.querySelector('#message-input');
const profileName = document.querySelector('#profile-name');
const profileEmail = document.querySelector('#profile-email');
const logoutButton = document.querySelector('#logout-button');
const recoveryDialog = document.querySelector('#recovery-dialog');
const recoveryPhrase = document.querySelector('#recovery-phrase');
const recoveryCopy = document.querySelector('#recovery-copy');
let restoreMode = false;
let currentIdentity;
let pollTimer;
let generatedIdentity;

const WORDS = ['amber', 'anchor', 'apple', 'arrow', 'atlas', 'autumn', 'bamboo', 'beacon', 'berry', 'blossom', 'blue', 'breeze', 'canyon', 'cedar', 'circle', 'cloud', 'cobalt', 'comet', 'coral', 'crystal', 'dawn', 'delta', 'ember', 'falcon', 'forest', 'glow', 'harbor', ' Hazel'.trim(), 'island', 'jasmine', 'lantern', 'lemon', 'linen', 'maple', 'meadow', 'meteor', 'mint', 'moon', 'navy', 'ocean', 'olive', 'orbit', 'pebble', 'pine', 'plum', 'prairie', 'rain', 'river', 'rose', 'saffron', 'shadow', 'silver', 'sky', 'snow', 'solar', 'sparrow', 'spring', 'stone', 'sunset', 'tulip', 'velvet', 'violet', 'willow', 'winter'];

function escapeHtml(value) {
  const element = document.createElement('span');
  element.textContent = value;
  return element.innerHTML;
}

function bytesToBase64(bytes) {
  let binary = '';
  bytes.forEach((byte) => { binary += String.fromCharCode(byte); });
  return btoa(binary);
}

function base64ToBytes(value) {
  return Uint8Array.from(atob(value), (character) => character.charCodeAt(0));
}

async function sha256Hex(value) {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('').slice(0, 32);
}

async function deriveRecoveryKey(phrase, salt) {
  const material = await crypto.subtle.importKey('raw', new TextEncoder().encode(phrase), 'PBKDF2', false, ['deriveKey']);
  return crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: 250000, hash: 'SHA-256' },
    material,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  );
}

async function encryptBundle(bundle, phrase) {
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await deriveRecoveryKey(phrase, salt);
  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    new TextEncoder().encode(JSON.stringify(bundle))
  );
  return JSON.stringify({ version: 1, kdf: 'PBKDF2-SHA256', salt: bytesToBase64(salt), iv: bytesToBase64(iv), ciphertext: bytesToBase64(new Uint8Array(ciphertext)) });
}

async function decryptBundle(serialized, phrase) {
  const bundle = JSON.parse(serialized);
  const key = await deriveRecoveryKey(phrase, base64ToBytes(bundle.salt));
  const plaintext = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: base64ToBytes(bundle.iv) }, key, base64ToBytes(bundle.ciphertext));
  return JSON.parse(new TextDecoder().decode(plaintext));
}

function createRecoveryPhrase() {
  const random = crypto.getRandomValues(new Uint8Array(12));
  return [...random].map((byte) => WORDS[byte % WORDS.length]).join(' ');
}

async function createIdentity() {
  const keyPair = await crypto.subtle.generateKey({ name: 'ECDH', namedCurve: 'P-256' }, true, ['deriveKey', 'deriveBits']);
  const publicKey = await crypto.subtle.exportKey('jwk', keyPair.publicKey);
  const privateKey = await crypto.subtle.exportKey('jwk', keyPair.privateKey);
  const phrase = createRecoveryPhrase();
  const publicKeyJson = JSON.stringify(publicKey);
  const identity = {
    accountId: await sha256Hex(publicKeyJson),
    publicKey: publicKeyJson,
    recoveryBundle: await encryptBundle({ privateKey, publicKey }, phrase)
  };
  return { identity, phrase };
}

function renderMessage(message) {
  const row = document.createElement('article');
  const name = message?.name || 'Anonymous';
  row.className = `message-row${message?.mine ? ' mine' : ''}`;
  row.innerHTML = `<div class="avatar ${message?.mine ? 'avatar-you' : 'avatar-maya'}">${escapeHtml(name[0].toUpperCase())}</div><div class="message"><div class="message-meta"><strong>${escapeHtml(name)}</strong><time>${escapeHtml(message?.time || 'now')}</time></div><p class="message-text">${escapeHtml(message?.text || '')}</p></div>`;
  messages.append(row);
}

async function api(path, options = {}) {
  const response = await fetch(path, { ...options, headers: { 'Content-Type': 'application/json', ...(options.headers || {}) } });
  let body = null;
  try { body = await response.json(); } catch (_) {}
  if (!response.ok) throw new Error(body?.message || body?.detail || `Request failed (${response.status})`);
  return body;
}

async function loadMessages(scroll = false) {
  const data = await api('/api/messages');
  messages.replaceChildren();
  data.forEach(renderMessage);
  if (scroll) messages.scrollTop = messages.scrollHeight;
}

function showApp(identity) {
  currentIdentity = identity;
  const shortId = identity.accountId.slice(0, 8);
  profileName.textContent = `anon-${shortId}`;
  profileEmail.textContent = identity.accountId;
  authScreen.hidden = true;
  appShell.hidden = false;
  loadMessages(true).catch(() => {});
  clearInterval(pollTimer);
  pollTimer = setInterval(() => loadMessages(false).catch(() => {}), 2000);
}

function showAuth() {
  clearInterval(pollTimer);
  appShell.hidden = true;
  authScreen.hidden = false;
}

function updateAuthMode() {
  restoreMode = !restoreMode;
  authTitle.textContent = restoreMode ? 'Restore your identity' : 'Create an anonymous identity';
  authPrompt.textContent = restoreMode ? 'Need a new identity?' : 'Already have an identity?';
  authSwitch.textContent = restoreMode ? 'Create one' : 'Restore it';
  nameField.hidden = true;
  accountIdInput.hidden = !restoreMode;
  authForm.querySelector('button[type="submit"]').textContent = restoreMode ? 'Restore identity' : 'Generate identity';
  authError.textContent = '';
}

authSwitch.addEventListener('click', updateAuthMode);
authForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  authError.textContent = '';
  try {
    if (!restoreMode) {
      generatedIdentity = await createIdentity();
      const registered = await api('/api/identity/register', { method: 'POST', body: JSON.stringify(generatedIdentity.identity) });
      recoveryPhrase.textContent = generatedIdentity.phrase;
      recoveryDialog.showModal();
      showApp(registered);
    } else {
      const response = await api('/api/identity/restore', { method: 'POST', body: JSON.stringify({ accountId: accountIdInput.value.trim().toLowerCase() }) });
      await decryptBundle(response.recoveryBundle, prompt('Enter your recovery phrase') || '');
      showApp(response);
    }
  } catch (error) {
    authError.textContent = error.message.includes('OperationError') ? 'That recovery phrase is incorrect.' : error.message;
  }
});

recoveryCopy.addEventListener('click', async () => {
  await navigator.clipboard.writeText(recoveryPhrase.textContent);
  recoveryCopy.textContent = 'Copied';
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
  } catch (_) {
    input.value = text;
  }
});

input.addEventListener('keydown', (event) => {
  if (event.key === 'Enter' && !event.shiftKey) form.requestSubmit();
});

api('/api/identity/me').then(showApp).catch(showAuth);
