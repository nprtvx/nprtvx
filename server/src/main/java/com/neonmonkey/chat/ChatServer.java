package com.neonmonkey.chat;

import jakarta.servlet.http.Cookie;
import jakarta.servlet.http.HttpServletRequest;
import jakarta.servlet.http.HttpServletResponse;
import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.server.ResponseStatusException;
import org.springframework.scheduling.annotation.EnableScheduling;
import org.springframework.scheduling.annotation.Scheduled;

import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.*;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;

@SpringBootApplication
@EnableScheduling
@RestController
public final class ChatServer {
    private static final String SESSION_COOKIE = "neonmonkey_session";
    private static final DateTimeFormatter TIME_FORMAT = DateTimeFormatter.ofPattern("HH:mm");
    private final Map<String, Identity> identities = new ConcurrentHashMap<>();
    private final Map<String, String> sessions = new ConcurrentHashMap<>();
    private final List<EncryptedMessage> messages = new CopyOnWriteArrayList<>();
    private final List<EncryptedAttachment> attachments = new CopyOnWriteArrayList<>();
    private final HttpClient httpClient = HttpClient.newHttpClient();
    private final Map<String, Group> groups = new ConcurrentHashMap<>();
    private final List<EncryptedGroupMessage> groupMessages = new CopyOnWriteArrayList<>();

    public static void main(String[] args) {
        SpringApplication.run(ChatServer.class, args);
    }

    @GetMapping("/health")
    public Map<String, String> health() {
        return Map.of("status", "ok");
    }

    @GetMapping({"/messages", "/settings"})
    public String appRoute() {
        return "forward:/index.html";
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
        String accountId = request.accountId().trim().toLowerCase(Locale.ROOT);
        Identity identity = identities.get(accountId);
        if (identity == null && request.publicKey() != null && request.recoveryBundle() != null
                && request.displayName() != null) {
            RegisterRequest registration = new RegisterRequest(accountId, request.displayName(),
                    request.publicKey(), request.recoveryBundle());
            validateRegistration(registration);
            identity = identities.computeIfAbsent(accountId,
                    ignored -> new Identity(accountId, request.displayName(), request.publicKey(), request.recoveryBundle()));
        }
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

    @GetMapping("/api/gifs/search")
    public List<GifResult> searchGifs(@RequestParam(defaultValue = "") String q,
                                      HttpServletRequest request) {
        requireIdentity(request);
        String providerUrl = System.getenv("GIF_PROVIDER_URL");
        String providerKey = System.getenv("GIF_PROVIDER_KEY");
        if (blank(q) || blank(providerUrl) || blank(providerKey)) return List.of();
        try {
            String encodedQuery = java.net.URLEncoder.encode(q.trim(), java.nio.charset.StandardCharsets.UTF_8);
            HttpRequest upstream = HttpRequest.newBuilder()
                    .uri(URI.create(providerUrl + "?q=" + encodedQuery + "&key=" + providerKey))
                    .GET().build();
            HttpResponse<String> response = httpClient.send(upstream, HttpResponse.BodyHandlers.ofString());
            if (response.statusCode() / 100 != 2) return List.of();
            return List.of(new GifResult("provider-response", response.body()));
        } catch (Exception exception) {
            return List.of();
        }
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
        long now = System.currentTimeMillis();
        return messages.stream()
                .filter(message -> message.expiresAt() == null || message.expiresAt() > now)
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
        Long expiresAt = expiry(request.expiresInSeconds());
        EncryptedMessage message = new EncryptedMessage(
                identity.accountId(), recipient.accountId(), request.iv(), request.ciphertext(),
                System.currentTimeMillis(), expiresAt);
        messages.add(message);
        return message;
    }

    @PostMapping("/api/direct/{recipientAccountId}/attachments")
    public EncryptedAttachment addDirectAttachment(@PathVariable String recipientAccountId,
                                                   @RequestBody(required = false) EncryptedAttachmentRequest request,
                                                   HttpServletRequest httpRequest) {
        Identity identity = requireIdentity(httpRequest);
        if (!identities.containsKey(recipientAccountId)) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, "Recipient identity not found");
        }
        EncryptedAttachment attachment = validateAttachment(request, identity.accountId(), recipientAccountId, null);
        attachments.add(attachment);
        return attachment;
    }

    @GetMapping("/api/direct/{recipientAccountId}/attachments")
    public List<EncryptedAttachment> getDirectAttachments(@PathVariable String recipientAccountId,
                                                          HttpServletRequest httpRequest) {
        Identity identity = requireIdentity(httpRequest);
        long now = System.currentTimeMillis();
        return attachments.stream()
                .filter(item -> (item.senderAccountId().equals(identity.accountId())
                        && item.recipientAccountId().equals(recipientAccountId))
                        || (item.senderAccountId().equals(recipientAccountId)
                        && item.recipientAccountId().equals(identity.accountId())))
                .filter(item -> item.expiresAt() == null || item.expiresAt() > now)
                .toList();
    }

    @PostMapping("/api/groups")
    public GroupResponse createGroup(@RequestBody(required = false) CreateGroupRequest request,
                                     HttpServletRequest httpRequest) {
        Identity owner = requireIdentity(httpRequest);
        if (request == null || blank(request.name()) || request.memberKeys() == null
                || !request.memberKeys().containsKey(owner.accountId())) {
            throw badRequest("Group name and an encrypted key for every member are required");
        }
        Set<String> members = new HashSet<>(request.memberKeys().keySet());
        members.add(owner.accountId());
        if (members.size() > 100 || request.name().trim().length() > 80) {
            throw badRequest("Group is too large or the name is too long");
        }
        for (String member : members) {
            if (!identities.containsKey(member) || blank(request.memberKeys().get(member))) {
                throw badRequest("Every group member must be a valid identity with an encrypted key");
            }
        }
        String groupId = UUID.randomUUID().toString();
        groups.put(groupId, new Group(groupId, request.name().trim(), owner.accountId(), members,
                Map.copyOf(request.memberKeys())));
        return groupResponse(groups.get(groupId), owner.accountId());
    }

    @GetMapping("/api/groups")
    public List<GroupResponse> listGroups(HttpServletRequest request) {
        Identity identity = requireIdentity(request);
        return groups.values().stream()
                .filter(group -> group.members().contains(identity.accountId()))
                .map(group -> groupResponse(group, identity.accountId()))
                .toList();
    }

    @GetMapping("/api/groups/{groupId}")
    public GroupResponse getGroup(@PathVariable String groupId, HttpServletRequest request) {
        Identity identity = requireIdentity(request);
        Group group = requireGroupMember(groupId, identity.accountId());
        return groupResponse(group, identity.accountId());
    }

    @GetMapping("/api/groups/{groupId}/messages")
    public List<EncryptedGroupMessage> getGroupMessages(@PathVariable String groupId,
                                                        HttpServletRequest request) {
        Identity identity = requireIdentity(request);
        requireGroupMember(groupId, identity.accountId());
        long now = System.currentTimeMillis();
        return groupMessages.stream().filter(message -> message.groupId().equals(groupId))
                .filter(message -> message.expiresAt() == null || message.expiresAt() > now).toList();
    }

    @PostMapping("/api/groups/{groupId}/messages")
    public EncryptedGroupMessage addGroupMessage(@PathVariable String groupId,
                                                 @RequestBody(required = false) EncryptedGroupMessageRequest request,
                                                 HttpServletRequest httpRequest) {
        Identity identity = requireIdentity(httpRequest);
        requireGroupMember(groupId, identity.accountId());
        if (request == null || blank(request.iv()) || blank(request.ciphertext())) {
            throw badRequest("Encrypted group message data is required");
        }
        if (request.iv().length() > 100 || request.ciphertext().length() > 20000) {
            throw badRequest("Encrypted group message data is too large");
        }
        Long expiresAt = expiry(request.expiresInSeconds());
        EncryptedGroupMessage message = new EncryptedGroupMessage(groupId, identity.accountId(),
                request.iv(), request.ciphertext(), System.currentTimeMillis(), expiresAt);
        groupMessages.add(message);
        return message;
    }

    @PostMapping("/api/groups/{groupId}/attachments")
    public EncryptedAttachment addGroupAttachment(@PathVariable String groupId,
                                                   @RequestBody(required = false) EncryptedAttachmentRequest request,
                                                   HttpServletRequest httpRequest) {
        Identity identity = requireIdentity(httpRequest);
        requireGroupMember(groupId, identity.accountId());
        EncryptedAttachment attachment = validateAttachment(request, identity.accountId(), null, groupId);
        attachments.add(attachment);
        return attachment;
    }

    @GetMapping("/api/groups/{groupId}/attachments")
    public List<EncryptedAttachment> getGroupAttachments(@PathVariable String groupId,
                                                         HttpServletRequest httpRequest) {
        Identity identity = requireIdentity(httpRequest);
        requireGroupMember(groupId, identity.accountId());
        long now = System.currentTimeMillis();
        return attachments.stream().filter(item -> groupId.equals(item.groupId()))
                .filter(item -> item.expiresAt() == null || item.expiresAt() > now).toList();
    }

    @Scheduled(fixedDelay = 60_000)
    public void deleteExpiredMessages() {
        long now = System.currentTimeMillis();
        messages.removeIf(message -> message.expiresAt() != null && message.expiresAt() <= now);
        groupMessages.removeIf(message -> message.expiresAt() != null && message.expiresAt() <= now);
        attachments.removeIf(item -> item.expiresAt() != null && item.expiresAt() <= now);
    }

    private EncryptedAttachment validateAttachment(EncryptedAttachmentRequest request, String sender,
                                                   String recipient, String groupId) {
        if (request == null || blank(request.iv()) || blank(request.ciphertext())
                || blank(request.name()) || blank(request.mimeType())) {
            throw badRequest("Encrypted attachment data is required");
        }
        if (request.ciphertext().length() > 15_000_000 || request.name().length() > 255
                || request.mimeType().length() > 120) {
            throw badRequest("Attachment is too large or has invalid metadata");
        }
        return new EncryptedAttachment(UUID.randomUUID().toString(), sender, recipient, groupId,
                request.name(), request.mimeType(), request.iv(), request.ciphertext(),
                System.currentTimeMillis(), expiry(request.expiresInSeconds()));
    }

    private static Long expiry(Integer seconds) {
        if (seconds == null || seconds == 0) return null;
        if (seconds < 10 || seconds > 2_592_000) {
            throw badRequest("Disappearing message duration must be between 10 seconds and 30 days");
        }
        return System.currentTimeMillis() + (seconds * 1000L);
    }

    private Group requireGroupMember(String groupId, String accountId) {
        Group group = groups.get(groupId);
        if (group == null || !group.members().contains(accountId)) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, "Group not found");
        }
        return group;
    }

    private GroupResponse groupResponse(Group group, String accountId) {
        return new GroupResponse(group.groupId(), group.name(), group.ownerAccountId(),
                group.memberKeys().get(accountId), group.members());
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
    private record Group(String groupId, String name, String ownerAccountId, Set<String> members,
                         Map<String, String> memberKeys) {}
    public record RegisterRequest(String accountId, String displayName, String publicKey, String recoveryBundle) {}
    public record RestoreRequest(String accountId, String displayName, String publicKey, String recoveryBundle) {}
    public record IdentityResponse(String accountId, String displayName, String publicKey, String recoveryBundle) {}
    public record PublicIdentity(String accountId, String displayName, String publicKey) {}
    public record EncryptedMessageRequest(String iv, String ciphertext, Integer expiresInSeconds) {}
    public record EncryptedMessage(String senderAccountId, String recipientAccountId, String iv,
                                   String ciphertext, long createdAt, Long expiresAt) {}
    public record CreateGroupRequest(String name, Map<String, String> memberKeys) {}
    public record GroupResponse(String groupId, String name, String ownerAccountId,
                                String encryptedGroupKey, Set<String> members) {}
    public record EncryptedGroupMessageRequest(String iv, String ciphertext, Integer expiresInSeconds) {}
    public record EncryptedGroupMessage(String groupId, String senderAccountId, String iv,
                                        String ciphertext, long createdAt, Long expiresAt) {}
    public record EncryptedAttachmentRequest(String name, String mimeType, String iv, String ciphertext,
                                             Integer expiresInSeconds) {}
    public record EncryptedAttachment(String attachmentId, String senderAccountId, String recipientAccountId,
                                      String groupId, String name, String mimeType, String iv,
                                      String ciphertext, long createdAt, Long expiresAt) {}
    public record GifResult(String id, String payload) {}
}
