const messages = document.querySelector('#messages');
const form = document.querySelector('#message-form');
const input = document.querySelector('#message-input');

function renderMessage(message) {
  const row = document.createElement('article');
  row.className = `message-row${message.mine ? ' mine' : ''}`;
  row.innerHTML = `<div class="avatar ${message.mine ? 'avatar-you' : 'avatar-maya'}">${message.name[0]}</div><div class="message"><div class="message-meta"><strong>${escapeHtml(message.name)}</strong><time>${message.time}</time></div><p class="message-text">${escapeHtml(message.text)}</p></div>`;
  messages.append(row);
}

function escapeHtml(value) {
  const element = document.createElement('span');
  element.textContent = value;
  return element.innerHTML;
}

async function loadMessages() {
  try {
    const response = await fetch('/api/messages');
    if (!response.ok) throw new Error('Unable to load messages');
    (await response.json()).forEach(renderMessage);
    messages.scrollTop = messages.scrollHeight;
  } catch (error) {
    renderMessage({ name: 'Gather', text: 'The server is offline. Start ChatServer.java to connect.', time: 'now', mine: false });
  }
}

form.addEventListener('submit', async (event) => {
  event.preventDefault();
  const text = input.value.trim();
  if (!text) return;
  input.value = '';
  try {
    const response = await fetch('/api/messages', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: 'You', text }) });
    if (!response.ok) throw new Error('Unable to send message');
    renderMessage(await response.json());
  } catch (error) {
    renderMessage({ name: 'You', text, time: 'local', mine: true });
  }
  messages.scrollTop = messages.scrollHeight;
});

input.addEventListener('keydown', (event) => {
  if (event.key === 'Enter' && !event.shiftKey) form.requestSubmit();
});

loadMessages();
