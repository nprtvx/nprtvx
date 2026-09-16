const authScreen = document.querySelector('#auth-screen');
const appShell = document.querySelector('#app-shell');
const authForm = document.querySelector('#auth-form');
const authSubmit = document.querySelector('#auth-submit');
const landingActions = document.querySelector('#landing-actions');
const authFormPanel = document.querySelector('#auth-form-panel');
const createAccountButton = document.querySelector('#create-account-button');
const restoreAccountButton = document.querySelector('#restore-account-button');
const backToLanding = document.querySelector('#back-to-landing');
const authError = document.querySelector('#auth-error');
const authTitle = document.querySelector('#auth-title');
const nameField = document.querySelector('#name-field');
const displayNameInput = document.querySelector('#display-name-input');
const usernameInput = document.querySelector('#username-input');
const passwordInput = document.querySelector('#password-input');
const messages = document.querySelector('#messages');
const form = document.querySelector('#message-form');
const input = document.querySelector('#message-input');
const expirySelect = document.querySelector('#expiry-select');
const attachButton = document.querySelector('#attach-button');
const attachmentInput = document.querySelector('#attachment-input');
const emojiBar = document.querySelector('#emoji-bar');
const profileName = document.querySelector('#profile-name');
const profileEmail = document.querySelector('#profile-email');
const logoutButton = document.querySelector('#logout-button');
const recipientForm = document.querySelector('#recipient-form');
const recipientInput = document.querySelector('#recipient-input');
const recipientError = document.querySelector('#recipient-error');
const chatList = document.querySelector('#chat-list');
const messagesTab = document.querySelector('#messages-tab');
const settingsTab = document.querySelector('#settings-tab');
const settingsPanel = document.querySelector('#settings-panel');
const settingsName = document.querySelector('#settings-name');
const settingsUsername = document.querySelector('#settings-username');
const settingsAccountId = document.querySelector('#settings-account-id');
const settingsLock = document.querySelector('#settings-lock');
const homeEmpty = document.querySelector('#home-empty');
const pageTitle = document.querySelector('#page-title');
let restoreMode = false;
let currentIdentity;
let pollTimer;
let generatedIdentity;

let currentPrivateKey;
let currentRecipient;
let currentRecipientKey;
let currentGroup;
let currentGroupKey;

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

async function importPrivateKey(jwk) {
  return crypto.subtle.importKey('jwk', jwk, { name: 'ECDH', namedCurve: 'P-256' }, true, ['deriveKey', 'deriveBits']);
}

async function importPublicKey(jwk) {
  return crypto.subtle.importKey('jwk', jwk, { name: 'ECDH', namedCurve: 'P-256' }, true, []);
}

async function restoreSessionPrivateKey() {
  const serialized = sessionStorage.getItem('neonmonkey_private_key');
  if (!serialized) return false;
  currentPrivateKey = await importPrivateKey(JSON.parse(serialized));
  return true;
}

async function rememberSessionPrivateKey(jwk) {
  sessionStorage.setItem('neonmonkey_private_key', JSON.stringify(jwk));
}

async function encryptMessage(text) {
  const key = await crypto.subtle.deriveKey(
    { name: 'ECDH', public: currentRecipientKey },
    currentPrivateKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  );
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    new TextEncoder().encode(text)
  );
  return { iv: bytesToBase64(iv), ciphertext: bytesToBase64(new Uint8Array(ciphertext)) };
}

async function decryptMessage(message, senderPublicKey) {
  const key = await crypto.subtle.deriveKey(
    { name: 'ECDH', public: senderPublicKey },
    currentPrivateKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['decrypt']
  );
  const plaintext = await crypto.subtle.decrypt(
    { name: 'AES-GCM', iv: base64ToBytes(message.iv) },
    key,
    base64ToBytes(message.ciphertext)
  );
  return new TextDecoder().decode(plaintext);
}

async function deriveSharedKey(publicKey) {
  return crypto.subtle.deriveKey(
    { name: 'ECDH', public: publicKey },
    currentPrivateKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  );
}

async function wrapGroupKey(groupKey, publicKey) {
  const sharedKey = await deriveSharedKey(publicKey);
  const rawKey = await crypto.subtle.exportKey('raw', groupKey);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, sharedKey, rawKey);
  return JSON.stringify({ iv: bytesToBase64(iv), ciphertext: bytesToBase64(new Uint8Array(ciphertext)) });
}

async function unwrapGroupKey(encryptedKey) {
  const envelope = JSON.parse(encryptedKey);
  const sharedKey = await deriveSharedKey(await importPublicKey(JSON.parse(currentIdentity.publicKey)));
  const rawKey = await crypto.subtle.decrypt(
    { name: 'AES-GCM', iv: base64ToBytes(envelope.iv) },
    sharedKey,
    base64ToBytes(envelope.ciphertext)
  );
  return crypto.subtle.importKey('raw', rawKey, { name: 'AES-GCM' }, false, ['encrypt', 'decrypt']);
}

async function encryptWithGroupKey(text) {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    currentGroupKey,
    new TextEncoder().encode(text)
  );
  return { iv: bytesToBase64(iv), ciphertext: bytesToBase64(new Uint8Array(ciphertext)) };
}

async function decryptWithGroupKey(message) {
  const plaintext = await crypto.subtle.decrypt(
    { name: 'AES-GCM', iv: base64ToBytes(message.iv) },
    currentGroupKey,
    base64ToBytes(message.ciphertext)
  );
  return new TextDecoder().decode(plaintext);
}

async function encryptAttachment(file) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = currentGroup ? currentGroupKey : await deriveSharedKey(currentRecipientKey);
  const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, bytes);
  return {
    name: file.name,
    mimeType: file.type || 'application/octet-stream',
    iv: bytesToBase64(iv),
    ciphertext: bytesToBase64(new Uint8Array(ciphertext)),
    expiresInSeconds: Number(expirySelect.value)
  };
}

async function createIdentity(displayName, password) {
  const keyPair = await crypto.subtle.generateKey({ name: 'ECDH', namedCurve: 'P-256' }, true, ['deriveKey', 'deriveBits']);
  const publicKey = await crypto.subtle.exportKey('jwk', keyPair.publicKey);
  const privateKey = await crypto.subtle.exportKey('jwk', keyPair.privateKey);
  const publicKeyJson = JSON.stringify(publicKey);
  const accountId = await sha256Hex(publicKeyJson);
  const identity = {
    accountId,
    displayName,
    publicKey: publicKeyJson,
    recoveryBundle: await encryptBundle({ privateKey, publicKey }, password)
  };
  return { identity, privateKey };
}

function renderMessage(message) {
  const row = document.createElement('article');
  const name = message?.name || 'Anonymous';
  row.className = `message-row${message?.mine ? ' mine' : ''}`;
  row.innerHTML = `<div class="avatar ${message?.mine ? 'avatar-you' : 'avatar-maya'}">${escapeHtml(name[0].toUpperCase())}</div><div class="message"><div class="message-meta"><strong>${escapeHtml(name)}</strong><time>${escapeHtml(message?.time || 'now')}</time></div><p class="message-text">${escapeHtml(message?.text || '')}</p></div>`;
  messages.append(row);
}

async function restoreServerSession() {
  return false;
}

async function api(path, options = {}, retried = false) {
  const response = await fetch(path, { ...options, headers: { 'Content-Type': 'application/json', ...(options.headers || {}) } });
  let body = null;
  try { body = await response.json(); } catch (_) {}
  if (response.status === 401 && !retried && path !== '/api/identity/restore'
      && await restoreServerSession()) {
    return api(path, options, true);
  }
  if (!response.ok) throw new Error(body?.message || body?.detail || `Request failed (${response.status})`);
  return body;
}

async function loadMessages(scroll = false) {
  if (currentGroup && currentGroupKey) {
    const data = await api(`/api/groups/${currentGroup.groupId}/messages`);
    messages.replaceChildren();
    for (const message of data) {
      renderMessage({
        name: message.senderAccountId === currentIdentity.accountId ? currentIdentity.displayName : 'Group member',
        text: await decryptWithGroupKey(message),
        time: new Date(message.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
        mine: message.senderAccountId === currentIdentity.accountId
      });
    }
    if (scroll) messages.scrollTop = messages.scrollHeight;
    return;
  }
  if (!currentRecipient || !currentRecipientKey || !currentPrivateKey) return;
  const data = await api(`/api/direct/${currentRecipient.accountId}`);
  messages.replaceChildren();
  for (const message of data) {
    const sentByMe = message.senderAccountId === currentIdentity.accountId;
    const sender = sentByMe ? currentIdentity : await api(`/api/identity/${message.senderAccountId}`);
    const keyOwner = sentByMe ? currentRecipient : sender;
    const text = await decryptMessage(message, await importPublicKey(JSON.parse(keyOwner.publicKey)));
    renderMessage({
      name: sender.displayName,
      text,
      time: new Date(message.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
      mine: sentByMe
    });
  }
  if (scroll) messages.scrollTop = messages.scrollHeight;
}

function navigate(path) {
  history.pushState({}, '', path);
  if (currentIdentity) {
    renderRoute(path);
  } else {
    renderAuthRoute(path);
  }
}

function renderRoute(path = window.location.pathname) {
  const settings = path === '/settings';
  if (settings) {
    pageTitle.textContent = 'Your profile';
    showSettingsTab();
  } else {
    pageTitle.textContent = 'Messages';
    showMessagesTab();
  }
}

function showApp(identity, path = '/messages') {
  currentIdentity = identity;
  if (window.location.pathname !== path) {
    history.replaceState({}, '', path);
  }
  const shortId = identity.accountId.slice(0, 8);
  profileName.textContent = identity.displayName || `anon-${shortId}`;
  profileEmail.textContent = identity.username ? `@${identity.username}` : identity.accountId;
  settingsName.textContent = identity.displayName;
  settingsUsername.textContent = identity.username ? `@${identity.username}` : 'Legacy account';
  settingsAccountId.textContent = identity.accountId;
  authScreen.hidden = true;
  appShell.hidden = false;
  renderRoute(path);
  renderRecentChats();
  loadMessages(true).catch(() => {});
  clearInterval(pollTimer);
  pollTimer = setInterval(() => loadMessages(false).catch(() => {}), 2000);
}

function renderRecentChats() {
  const chats = JSON.parse(localStorage.getItem('neonmonkey_chats') || '[]');
  chatList.replaceChildren();
  if (!chats.length) {
    chatList.innerHTML = '<button class="channel empty-chat" type="button"><i>↗</i> No chats yet</button>';
    return;
  }
  chats.forEach((chat) => {
    const button = document.createElement('button');
    button.className = 'channel';
    button.type = 'button';
    button.innerHTML = `<i>↗</i> ${escapeHtml(chat.name)}`;
    button.addEventListener('click', () => openDirectChat(chat.accountId).catch(() => {}));
    chatList.append(button);
  });
}

async function openDirectChat(accountId) {
  const recipient = await api(`/api/identity/${accountId}`);
  currentRecipient = recipient;
  currentGroup = null;
  currentGroupKey = null;
  currentRecipientKey = await importPublicKey(JSON.parse(recipient.publicKey));
  input.disabled = false;
  input.placeholder = `Message ${recipient.displayName}`;
  homeEmpty.hidden = true;
  const chats = JSON.parse(localStorage.getItem('neonmonkey_chats') || '[]')
    .filter((chat) => chat.accountId !== recipient.accountId);
  chats.unshift({ accountId: recipient.accountId, name: recipient.displayName });
  localStorage.setItem('neonmonkey_chats', JSON.stringify(chats.slice(0, 50)));
  renderRecentChats();
  await loadMessages(true);
}

function showAuth() {
  clearInterval(pollTimer);
  sessionStorage.removeItem('neonmonkey_private_key');
  currentIdentity = undefined;
  currentPrivateKey = undefined;
  currentRecipient = undefined;
  currentRecipientKey = undefined;
  currentGroup = undefined;
  currentGroupKey = undefined;
  appShell.hidden = true;
  authScreen.hidden = false;
  landingActions.hidden = false;
  authFormPanel.hidden = true;
  authForm.reset();
  renderAuthRoute(window.location.pathname);
}

function renderAuthRoute(path = window.location.pathname) {
  if (path === '/create' || path === '/restore') {
    openAuth(path === '/restore' ? 'restore' : 'create');
  } else {
    landingActions.hidden = false;
    authFormPanel.hidden = true;
  }
}

function openAuth(mode) {
  restoreMode = mode === 'restore';
  landingActions.hidden = true;
  authFormPanel.hidden = false;
  authTitle.textContent = restoreMode ? 'Restore your account' : 'Create your account';
  nameField.hidden = restoreMode;
  displayNameInput.required = !restoreMode;
  passwordInput.autocomplete = restoreMode ? 'current-password' : 'new-password';
  authSubmit.textContent = restoreMode ? 'Log in' : 'Create account';
  authError.textContent = '';
}

createAccountButton.addEventListener('click', () => navigate('/create'));
restoreAccountButton.addEventListener('click', () => navigate('/restore'));
backToLanding.addEventListener('click', () => navigate('/'));
authForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  authError.textContent = '';
  authSubmit.disabled = true;
  authSubmit.textContent = restoreMode ? 'Logging in…' : 'Creating account…';
  try {
    const username = usernameInput.value.trim().toLowerCase();
    const password = passwordInput.value;
    if (!/^[a-z0-9_]{3,24}$/.test(username)) {
      throw new Error('Username must be 3-24 characters using letters, numbers, or underscores');
    }
    if (password.length < 8) throw new Error('Password must be at least 8 characters');
    if (!restoreMode) {
      const displayName = displayNameInput.value.trim();
      if (!displayName) throw new Error('Enter a display name');
      generatedIdentity = await createIdentity(displayName, password);
      currentPrivateKey = await importPrivateKey(generatedIdentity.privateKey);
      await rememberSessionPrivateKey(generatedIdentity.privateKey);
      const registered = await api('/api/auth/register', {
        method: 'POST',
        body: JSON.stringify({ username, password, displayName, ...generatedIdentity.identity })
      });
      localStorage.setItem('neonmonkey_identity', JSON.stringify({
        accountId: registered.accountId,
        username: registered.username,
        publicKey: registered.publicKey,
        recoveryBundle: registered.recoveryBundle,
        displayName: registered.displayName
      }));
      window.location.assign('/settings');
      return;
    } else {
      let response;
      try {
        response = await api('/api/auth/login', {
          method: 'POST',
          body: JSON.stringify({ username, password })
        });
      } catch (loginError) {
        const saved = JSON.parse(localStorage.getItem('neonmonkey_identity') || 'null');
        if (loginError.message !== 'Account not found'
            || saved?.username !== username || !saved?.accountId || !saved?.recoveryBundle) {
          throw loginError;
        }
        response = await api('/api/identity/restore', {
          method: 'POST',
          body: JSON.stringify({
            accountId: saved.accountId,
            username,
            password,
            displayName: saved.displayName,
            publicKey: saved.publicKey,
            recoveryBundle: saved.recoveryBundle
          })
        });
      }
      const bundle = await decryptBundle(response.recoveryBundle, password);
      currentPrivateKey = await importPrivateKey(bundle.privateKey);
      await rememberSessionPrivateKey(bundle.privateKey);
      localStorage.setItem('neonmonkey_identity', JSON.stringify({
        accountId: response.accountId,
        username: response.username,
        displayName: response.displayName,
        publicKey: response.publicKey,
        recoveryBundle: response.recoveryBundle
      }));
      window.location.assign('/messages');
      return;
    }
  } catch (error) {
    authError.textContent = error.message.includes('OperationError')
      ? 'That password could not unlock this account.'
      : error.message || 'Login failed. Try again.';
  } finally {
    authSubmit.disabled = false;
    authSubmit.textContent = restoreMode ? 'Log in' : 'Create account';
  }
});

recipientForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  recipientError.textContent = '';
  const accountId = recipientInput.value.trim().toLowerCase();
  if (!/^[a-f0-9]{32}$/.test(accountId)) {
    recipientError.textContent = 'Enter a valid 32-character account ID.';
    return;
  }
  try {
    await openDirectChat(accountId);
  } catch (error) {
    recipientError.textContent = error.message === 'Request failed (404)'
      ? 'Recipient ID was not found on this NeonMonkey server. Confirm the ID and ask the recipient to create or restore the account here.'
      : error.message;
  }
});

function showMessagesTab() {
  messagesTab.classList.add('active');
  settingsTab.classList.remove('active');
  settingsPanel.hidden = true;
  homeEmpty.hidden = Boolean(currentRecipient || currentGroup);
}

function showSettingsTab() {
  settingsTab.classList.add('active');
  messagesTab.classList.remove('active');
  settingsPanel.hidden = false;
}

messagesTab.addEventListener('click', showMessagesTab);
settingsTab.addEventListener('click', showSettingsTab);
messagesTab.addEventListener('click', () => navigate('/messages'));
settingsTab.addEventListener('click', () => navigate('/settings'));
window.addEventListener('popstate', () => {
  if (currentIdentity) {
    renderRoute();
  } else {
    renderAuthRoute();
  }
});
settingsLock.addEventListener('click', async () => {
  await api('/api/auth/logout', { method: 'POST' }).catch(() => {});
  showAuth();
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
    if (currentGroup) {
      await api(`/api/groups/${currentGroup.groupId}/messages`, { method: 'POST', body: JSON.stringify({
        ...await encryptWithGroupKey(text), expiresInSeconds: Number(expirySelect.value)
      }) });
    } else {
      if (!currentRecipient) throw new Error('Select a conversation first');
      await api(`/api/direct/${currentRecipient.accountId}`, { method: 'POST', body: JSON.stringify({
        ...await encryptMessage(text), expiresInSeconds: Number(expirySelect.value)
      }) });
    }
    await loadMessages(true);
  } catch (error) {
    input.value = text;
    input.placeholder = error.message || 'Message could not be sent';
  }
});

input.addEventListener('keydown', (event) => {
  if (event.key === 'Enter' && !event.shiftKey) form.requestSubmit();
});

attachButton.addEventListener('click', () => attachmentInput.click());
emojiBar.addEventListener('click', (event) => {
  const button = event.target.closest('button');
  if (!button || input.disabled) return;
  const start = input.selectionStart ?? input.value.length;
  input.value = `${input.value.slice(0, start)}${button.textContent}${input.value.slice(start)}`;
  input.focus();
  input.selectionStart = input.selectionEnd = start + button.textContent.length;
});
attachmentInput.addEventListener('change', async () => {
  const file = attachmentInput.files?.[0];
  attachmentInput.value = '';
  if (!file || (!currentRecipient && !currentGroup)) return;
  try {
    const payload = await encryptAttachment(file);
    const path = currentGroup
      ? `/api/groups/${currentGroup.groupId}/attachments`
      : `/api/direct/${currentRecipient.accountId}/attachments`;
    await api(path, { method: 'POST', body: JSON.stringify(payload) });
    input.placeholder = `${file.name} uploaded encrypted`;
  } catch (error) {
    input.placeholder = error.message;
  }
});

api('/api/identity/me')
  .then(async (identity) => {
    const hasPrivateKey = await restoreSessionPrivateKey();
    if (!hasPrivateKey) {
      showAuth();
      navigate('/restore');
      return;
    }
    showApp(identity, window.location.pathname);
  })
  .catch(() => {
    showAuth();
    renderAuthRoute();
  });
