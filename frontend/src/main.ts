import { CommonModule } from '@angular/common';
import { bootstrapApplication } from '@angular/platform-browser';
import { Component, EventEmitter, forwardRef, Input, OnDestroy, OnInit, Output } from '@angular/core';
import { FormsModule } from '@angular/forms';

type Identity = {
  accountId: string;
  username: string;
  displayName: string;
  publicKey: string;
  recoveryBundle: string;
};

type Chat = { accountId: string; name: string };
type Message = { id: string; name: string; text: string; time: string; mine: boolean };

const IDENTITY_KEY = 'neonmonkey_identity';
const PRIVATE_KEY = 'neonmonkey_private_key';

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const response = await fetch(path, {
    ...options,
    headers: { 'Content-Type': 'application/json', ...(options.headers ?? {}) }
  });
  let body: { message?: string } | null = null;
  try { body = await response.json(); } catch (_) {}
  if (!response.ok) throw new Error(body?.message ?? `Request failed (${response.status})`);
  return body as T;
}

function b64(bytes: Uint8Array): string {
  let value = '';
  bytes.forEach((byte) => value += String.fromCharCode(byte));
  return btoa(value);
}

function bytes(value: string): Uint8Array {
  return Uint8Array.from(atob(value), (character) => character.charCodeAt(0));
}

async function keyFromPassword(password: string, salt: Uint8Array): Promise<CryptoKey> {
  const material = await crypto.subtle.importKey('raw', new TextEncoder().encode(password), 'PBKDF2', false, ['deriveKey']);
  return crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: 250000, hash: 'SHA-256' },
    material, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']
  );
}

async function encryptBundle(bundle: object, password: string): Promise<string> {
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const encrypted = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv }, await keyFromPassword(password, salt),
    new TextEncoder().encode(JSON.stringify(bundle))
  );
  return JSON.stringify({ version: 1, salt: b64(salt), iv: b64(iv), ciphertext: b64(new Uint8Array(encrypted)) });
}

async function decryptBundle(serialized: string, password: string): Promise<{ privateKey: JsonWebKey }> {
  const bundle = JSON.parse(serialized) as { salt: string; iv: string; ciphertext: string };
  const plaintext = await crypto.subtle.decrypt(
    { name: 'AES-GCM', iv: bytes(bundle.iv) }, await keyFromPassword(password, bytes(bundle.salt)), bytes(bundle.ciphertext)
  );
  return JSON.parse(new TextDecoder().decode(plaintext)) as { privateKey: JsonWebKey };
}

async function importPrivate(jwk: JsonWebKey): Promise<CryptoKey> {
  return crypto.subtle.importKey('jwk', jwk, { name: 'ECDH', namedCurve: 'P-256' }, true, ['deriveKey']);
}

async function importPublic(jwk: JsonWebKey): Promise<CryptoKey> {
  return crypto.subtle.importKey('jwk', jwk, { name: 'ECDH', namedCurve: 'P-256' }, true, []);
}

async function makeIdentity(displayName: string, username: string, password: string): Promise<{ data: object; privateKey: JsonWebKey }> {
  const pair = await crypto.subtle.generateKey({ name: 'ECDH', namedCurve: 'P-256' }, true, ['deriveKey']);
  const publicKey = await crypto.subtle.exportKey('jwk', pair.publicKey);
  const privateKey = await crypto.subtle.exportKey('jwk', pair.privateKey);
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(JSON.stringify(publicKey)));
  const accountId = [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('').slice(0, 32);
  return {
    privateKey,
    data: {
      accountId, username, displayName, publicKey: JSON.stringify(publicKey),
      recoveryBundle: await encryptBundle({ privateKey, publicKey }, password)
    }
  };
}

async function encryptMessage(text: string, privateKey: CryptoKey, publicKey: CryptoKey): Promise<{ iv: string; ciphertext: string }> {
  const key = await crypto.subtle.deriveKey({ name: 'ECDH', public: publicKey }, privateKey, { name: 'AES-GCM', length: 256 }, false, ['encrypt']);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const encrypted = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, new TextEncoder().encode(text));
  return { iv: b64(iv), ciphertext: b64(new Uint8Array(encrypted)) };
}

async function decryptMessage(message: { iv: string; ciphertext: string }, privateKey: CryptoKey, publicKey: CryptoKey): Promise<string> {
  const key = await crypto.subtle.deriveKey({ name: 'ECDH', public: publicKey }, privateKey, { name: 'AES-GCM', length: 256 }, false, ['decrypt']);
  const plaintext = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: bytes(message.iv) }, key, bytes(message.ciphertext));
  return new TextDecoder().decode(plaintext);
}

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [CommonModule, FormsModule, forwardRef(() => AuthComponent), forwardRef(() => ShellComponent)],
  template: `
    <ng-container *ngIf="loading; else loaded">
      <main class="loading"><div class="brand"><span class="brand-mark">nm</span> neonmonkey</div><span>Loading your private space…</span></main>
    </ng-container>
    <ng-template #loaded>
      <app-auth *ngIf="!identity" (authenticated)="setSession($event)"></app-auth>
      <app-shell *ngIf="identity" [identity]="identity" [privateKey]="privateKey!" (logout)="logout()"></app-shell>
    </ng-template>
  `
})
class RootComponent implements OnInit {
  identity?: Identity;
  privateKey?: CryptoKey;
  loading = true;

  async ngOnInit(): Promise<void> {
    try {
      const storedKey = sessionStorage.getItem(PRIVATE_KEY);
      if (storedKey) {
        this.privateKey = await importPrivate(JSON.parse(storedKey) as JsonWebKey);
        this.identity = await request<Identity>('/api/identity/me');
      }
    } catch (_) {
      this.identity = undefined;
      this.privateKey = undefined;
    } finally {
      this.loading = false;
    }
  }

  async setSession(event: { identity: Identity; privateKey: JsonWebKey }): Promise<void> {
    sessionStorage.setItem(PRIVATE_KEY, JSON.stringify(event.privateKey));
    this.privateKey = await importPrivate(event.privateKey);
    this.identity = event.identity;
    localStorage.setItem(IDENTITY_KEY, JSON.stringify(event.identity));
  }

  logout(): void {
    void request('/api/auth/logout', { method: 'POST' }).catch(() => {});
    sessionStorage.removeItem(PRIVATE_KEY);
    this.identity = undefined;
    this.privateKey = undefined;
  }
}

@Component({
  selector: 'app-auth',
  standalone: true,
  imports: [CommonModule, FormsModule],
  template: `
    <main class="auth-page"><section class="auth-card">
      <div class="brand auth-brand"><span class="brand-mark">nm</span> neonmonkey</div>
      <ng-container *ngIf="mode === 'landing'; else form">
        <div class="hero"><span class="eyebrow">PRIVATE MESSAGING, REIMAGINED</span><h1>Speak freely.<br><em>Stay unknown.</em></h1><p>Private conversations with a username and password. Your encryption keys stay in your browser.</p></div>
        <button class="primary" (click)="mode = 'create'">Create account <span>→</span></button>
        <button class="secondary" (click)="mode = 'login'">Log in</button>
      </ng-container>
      <ng-template #form>
        <button class="back" (click)="mode = 'landing'">← Back</button>
        <span class="eyebrow">YOUR PRIVATE ACCOUNT</span><h1>{{ mode === 'create' ? 'Create your account' : 'Welcome back' }}</h1>
        <p class="muted">{{ mode === 'create' ? 'Choose a username and password to create your encrypted identity.' : 'Log in to unlock your encrypted identity.' }}</p>
        <form (ngSubmit)="submit()">
          <label>Username<input name="username" [(ngModel)]="username" autocomplete="username" placeholder="e.g. luna_7" required></label>
          <label>Password<input name="password" [(ngModel)]="password" type="password" autocomplete="current-password" placeholder="At least 8 characters" required></label>
          <label *ngIf="mode === 'create'">Display name<input name="displayName" [(ngModel)]="displayName" placeholder="e.g. Luna" required></label>
          <p class="error" *ngIf="error">{{ error }}</p><button class="primary" [disabled]="busy">{{ busy ? 'Working…' : mode === 'create' ? 'Create account' : 'Log in' }}</button>
        </form>
      </ng-template>
    </section></main>
  `
})
class AuthComponent {
  @Output() authenticated = new EventEmitter<{ identity: Identity; privateKey: JsonWebKey }>();
  mode: 'landing' | 'create' | 'login' = 'landing';
  username = ''; password = ''; displayName = ''; error = ''; busy = false;
  async submit(): Promise<void> {
    this.error = '';
    const username = this.username.trim().toLowerCase();
    if (!/^[a-z0-9_]{3,24}$/.test(username)) { this.error = 'Use 3-24 lowercase letters, numbers, or underscores.'; return; }
    if (this.password.length < 8) { this.error = 'Password must be at least 8 characters.'; return; }
    this.busy = true;
    try {
      let identity: Identity;
      let privateKey: JsonWebKey;
      if (this.mode === 'create') {
        if (!this.displayName.trim()) throw new Error('Enter a display name.');
        const generated = await makeIdentity(this.displayName.trim(), username, this.password);
        identity = await request<Identity>('/api/auth/register', { method: 'POST', body: JSON.stringify(generated.data) });
        privateKey = generated.privateKey;
      } else {
        identity = await request<Identity>('/api/auth/login', { method: 'POST', body: JSON.stringify({ username, password: this.password }) });
        privateKey = (await decryptBundle(identity.recoveryBundle, this.password)).privateKey;
      }
      sessionStorage.setItem(PRIVATE_KEY, JSON.stringify(privateKey));
      localStorage.setItem(IDENTITY_KEY, JSON.stringify(identity));
      this.authenticated.emit({ identity, privateKey });
    } catch (caught) {
      this.error = caught instanceof Error ? caught.message : 'Authentication failed.';
    } finally { this.busy = false; }
  }
}

@Component({
  selector: 'app-shell',
  standalone: true,
  imports: [CommonModule, FormsModule, forwardRef(() => SettingsComponent)],
  template: `
    <div class="app"><aside class="sidebar"><div class="brand"><span class="brand-mark">nm</span> neonmonkey</div>
      <nav><button class="nav" [class.active]="page === 'messages'" (click)="page = 'messages'">▦ <span>Messages</span></button><button class="nav" [class.active]="page === 'settings'" (click)="page = 'settings'">⚙ <span>Settings</span></button></nav>
      <span class="eyebrow chats-label">YOUR CHATS</span><div class="chat-list"><button class="chat-link" *ngFor="let chat of chats" (click)="openChat(chat.accountId)"><i>↗</i>{{ chat.name }}</button><span class="empty" *ngIf="!chats.length">No chats yet</span></div>
      <div class="profile"><div class="avatar">{{ initial(identity.displayName) }}</div><div><strong>{{ identity.displayName }}</strong><small>&#64;{{ identity.username }}</small></div><button class="lock" (click)="logout.emit()">Log out</button></div>
    </aside><main class="main"><header><span class="eyebrow">NEONMONKEY</span><h1>{{ page === 'settings' ? 'Your profile' : recipient?.displayName || 'Messages' }}</h1><p>Private conversations, encrypted on your device.</p></header>
      <app-settings *ngIf="page === 'settings'" [identity]="identity" (logout)="logout.emit()"></app-settings>
      <ng-container *ngIf="page === 'messages'"><section class="messages" *ngIf="recipient; else emptyState"><article class="message" [class.mine]="message.mine" *ngFor="let message of messages"><div class="avatar">{{ initial(message.name) }}</div><div><div class="meta"><strong>{{ message.name }}</strong><time>{{ message.time }}</time></div><p>{{ message.text }}</p></div></article></section><form class="composer" *ngIf="recipient" (ngSubmit)="send()"><input name="draft" [(ngModel)]="draft" [placeholder]="'Message ' + recipient.displayName"><button class="send" [disabled]="busy">↑</button></form></ng-container>
      <ng-template #emptyState><section class="empty-state"><div class="monkey">🐒</div><h2>Your messages</h2><p>Start a private conversation by adding someone’s NeonMonkey ID.</p><form class="recipient" (ngSubmit)="startChat()"><input name="recipientId" [(ngModel)]="recipientId" placeholder="Paste a recipient ID" required><button class="primary">Start chat</button></form><p class="error" *ngIf="error">{{ error }}</p></section></ng-template>
    </main></div>
  `
})
class ShellComponent implements OnDestroy {
  @Input() identity!: Identity;
  @Input() privateKey!: CryptoKey;
  @Output() logout = new EventEmitter<void>();
  page = location.pathname === '/settings' ? 'settings' : 'messages'; chats: Chat[] = JSON.parse(localStorage.getItem('neonmonkey_chats') || '[]');
  recipient?: Identity; recipientId = ''; error = ''; draft = ''; busy = false; messages: Message[] = []; timer?: number;
  initial(value: string): string { return (value || '?').slice(0, 1).toUpperCase(); }
  async startChat(): Promise<void> { try { await this.openChat(this.recipientId); this.recipientId = ''; } catch (caught) { this.error = caught instanceof Error ? caught.message : 'Recipient not found.'; } }
  async openChat(id: string): Promise<void> { this.recipient = await request<Identity>(`/api/identity/${id.trim().toLowerCase()}`); this.chats = [{ accountId: this.recipient.accountId, name: this.recipient.displayName }, ...this.chats.filter((chat) => chat.accountId !== this.recipient?.accountId)]; localStorage.setItem('neonmonkey_chats', JSON.stringify(this.chats)); this.refresh(); if (!this.timer) this.timer = window.setInterval(() => this.refresh(), 2500); }
  async refresh(): Promise<void> { if (!this.recipient) return; const data = await request<Array<{ senderAccountId: string; iv: string; ciphertext: string; createdAt: number }>>(`/api/direct/${this.recipient.accountId}`); this.messages = []; for (const item of data) { const sender = item.senderAccountId === this.identity.accountId ? this.identity : await request<Identity>(`/api/identity/${item.senderAccountId}`); this.messages.push({ id: `${item.createdAt}-${item.senderAccountId}`, name: sender.displayName, text: await decryptMessage(item, this.privateKey, await importPublic(JSON.parse(sender.publicKey))), time: new Date(item.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }), mine: item.senderAccountId === this.identity.accountId }); } }
  async send(): Promise<void> { if (!this.recipient || !this.draft.trim() || this.busy) return; this.busy = true; try { const payload = await encryptMessage(this.draft.trim(), this.privateKey, await importPublic(JSON.parse(this.recipient.publicKey))); await request(`/api/direct/${this.recipient.accountId}`, { method: 'POST', body: JSON.stringify(payload) }); this.draft = ''; await this.refresh(); } catch (caught) { this.error = caught instanceof Error ? caught.message : 'Message could not be sent.'; } finally { this.busy = false; } }
  ngOnDestroy(): void { if (this.timer) window.clearInterval(this.timer); }
}

@Component({ selector: 'app-settings', standalone: true, imports: [CommonModule], template: `<section class="settings"><div class="settings-icon">⚙</div><h2>{{ identity.displayName }}</h2><p class="muted">Your account uses a username and password. Your encryption keys remain protected in this browser.</p><div class="setting"><span class="eyebrow">USERNAME</span><code>&#64;{{ identity.username }}</code></div><div class="setting"><span class="eyebrow">ACCOUNT ID</span><code>{{ identity.accountId }}</code></div><button class="primary" (click)="logout.emit()">Log out</button></section>` })
class SettingsComponent { @Input() identity!: Identity; @Output() logout = new EventEmitter<void>(); }

bootstrapApplication(RootComponent).catch((error) => console.error(error));
