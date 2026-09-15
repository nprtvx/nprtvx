package com.neonmonkey.chat;

import jakarta.servlet.http.Cookie;
import jakarta.servlet.http.HttpServletRequest;
import jakarta.servlet.http.HttpServletResponse;
import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.server.ResponseStatusException;

import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.*;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;

@SpringBootApplication
@RestController
public final class ChatServer {
    private static final String SESSION_COOKIE = "gather_session";
    private static final DateTimeFormatter TIME_FORMAT = DateTimeFormatter.ofPattern("HH:mm");
    private final Map<String, Identity> identities = new ConcurrentHashMap<>();
    private final Map<String, String> sessions = new ConcurrentHashMap<>();
    private final List<EncryptedMessage> messages = new CopyOnWriteArrayList<>();

    public static void main(String[] args) {
        SpringApplication.run(ChatServer.class, args);
    }

    @GetMapping("/health")
    public Map<String, String> health() {
        return Map.of("status", "ok");
    }

    @PostMapping("/api/identity/register")
    public IdentityResponse register(@RequestBody(required = false) RegisterRequest request,
                                     HttpServletResponse response) {
        validateRegistration(request);
        if (identities.putIfAbsent(request.accountId(), new Identity(
                request.accountId(), request.displayName(), request.publicKey(), request.recoveryBundle())) != null) {
            throw new ResponseStatusException(HttpStatus.CONFLICT, "That identity already exists");
        }
        createSession(request.accountId(), response);
        return new IdentityResponse(request.accountId(), request.displayName(), request.publicKey(), request.recoveryBundle());
    }

    @PostMapping("/api/identity/restore")
    public IdentityResponse restore(@RequestBody(required = false) RestoreRequest request,
                                    HttpServletResponse response) {
        if (request == null || blank(request.accountId())) {
            throw badRequest("Account ID is required");
        }
        Identity identity = identities.get(request.accountId().trim().toLowerCase(Locale.ROOT));
        if (identity == null) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, "Identity not found");
        }
        createSession(identity.accountId(), response);
        return new IdentityResponse(identity.accountId(), identity.displayName(), identity.publicKey(), identity.recoveryBundle());
    }

    @GetMapping("/api/identity/me")
    public IdentityResponse currentIdentity(HttpServletRequest request) {
        Identity identity = requireIdentity(request);
        return new IdentityResponse(identity.accountId(), identity.displayName(), identity.publicKey(), identity.recoveryBundle());
    }

    @GetMapping("/api/identity/{accountId}")
    public PublicIdentity findIdentity(@PathVariable String accountId, HttpServletRequest request) {
        requireIdentity(request);
        Identity identity = identities.get(accountId.toLowerCase(Locale.ROOT));
        if (identity == null) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, "Identity not found");
        }
        return new PublicIdentity(identity.accountId(), identity.displayName(), identity.publicKey());
    }

    @PostMapping("/api/auth/logout")
    public void logout(HttpServletRequest request, HttpServletResponse response) {
        String token = cookieValue(request, SESSION_COOKIE);
        if (token != null) sessions.remove(token);
        Cookie cookie = new Cookie(SESSION_COOKIE, "");
        cookie.setMaxAge(0);
        cookie.setPath("/");
        response.addCookie(cookie);
    }

    @GetMapping("/api/direct/{recipientAccountId}")
    public List<EncryptedMessage> getDirectMessages(@PathVariable String recipientAccountId,
                                                    HttpServletRequest request) {
        Identity identity = requireIdentity(request);
        return messages.stream()
                .filter(message -> (message.senderAccountId().equals(identity.accountId())
                        && message.recipientAccountId().equals(recipientAccountId))
                        || (message.senderAccountId().equals(recipientAccountId)
                        && message.recipientAccountId().equals(identity.accountId())))
                .toList();
    }

    @PostMapping("/api/direct/{recipientAccountId}")
    public EncryptedMessage addDirectMessage(@PathVariable String recipientAccountId,
                                             @RequestBody(required = false) EncryptedMessageRequest request,
                                             HttpServletRequest httpRequest) {
        Identity identity = requireIdentity(httpRequest);
        Identity recipient = identities.get(recipientAccountId.toLowerCase(Locale.ROOT));
        if (recipient == null) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, "Recipient identity not found");
        }
        if (request == null || blank(request.iv()) || blank(request.ciphertext())) {
            throw badRequest("Encrypted message data is required");
        }
        if (request.iv().length() > 100 || request.ciphertext().length() > 10000) {
            throw badRequest("Encrypted message data is too large");
        }
        EncryptedMessage message = new EncryptedMessage(
                identity.accountId(), recipient.accountId(), request.iv(), request.ciphertext(),
                System.currentTimeMillis());
        messages.add(message);
        return message;
    }

    private void validateRegistration(RegisterRequest request) {
        if (request == null || blank(request.accountId()) || blank(request.displayName()) || blank(request.publicKey())
                || blank(request.recoveryBundle())) {
            throw badRequest("Generated identity data is incomplete");
        }
        if (request.displayName().trim().length() > 40) {
            throw badRequest("Display name cannot exceed 40 characters");
        }
        if (!request.accountId().matches("[a-f0-9]{32}")) {
            throw badRequest("Invalid account ID");
        }
        if (request.publicKey().length() > 10000 || request.recoveryBundle().length() > 20000) {
            throw badRequest("Identity data is too large");
        }
    }

    private Identity requireIdentity(HttpServletRequest request) {
        String token = cookieValue(request, SESSION_COOKIE);
        String accountId = token == null ? null : sessions.get(token);
        Identity identity = accountId == null ? null : identities.get(accountId);
        if (identity == null) {
            throw new ResponseStatusException(HttpStatus.UNAUTHORIZED, "Restore your identity to continue");
        }
        return identity;
    }

    private void createSession(String accountId, HttpServletResponse response) {
        String token = UUID.randomUUID().toString();
        sessions.put(token, accountId);
        Cookie cookie = new Cookie(SESSION_COOKIE, token);
        cookie.setHttpOnly(true);
        cookie.setSecure(true);
        cookie.setMaxAge(60 * 60 * 24 * 30);
        cookie.setPath("/");
        response.addCookie(cookie);
    }

    private static String cookieValue(HttpServletRequest request, String name) {
        if (request.getCookies() == null) return null;
        return Arrays.stream(request.getCookies())
                .filter(cookie -> name.equals(cookie.getName()))
                .map(Cookie::getValue)
                .findFirst().orElse(null);
    }

    private static boolean blank(String value) {
        return value == null || value.isBlank();
    }

    private static ResponseStatusException badRequest(String message) {
        return new ResponseStatusException(HttpStatus.BAD_REQUEST, message);
    }

    private record Identity(String accountId, String displayName, String publicKey, String recoveryBundle) {}
    public record RegisterRequest(String accountId, String displayName, String publicKey, String recoveryBundle) {}
    public record RestoreRequest(String accountId) {}
    public record IdentityResponse(String accountId, String displayName, String publicKey, String recoveryBundle) {}
    public record PublicIdentity(String accountId, String displayName, String publicKey) {}
    public record EncryptedMessageRequest(String iv, String ciphertext) {}
    public record EncryptedMessage(String senderAccountId, String recipientAccountId, String iv,
                                   String ciphertext, long createdAt) {}
}
