package com.neonmonkey.chat;

import jakarta.servlet.http.Cookie;
import jakarta.servlet.http.HttpServletRequest;
import jakarta.servlet.http.HttpServletResponse;
import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.server.ResponseStatusException;

import javax.crypto.SecretKeyFactory;
import javax.crypto.spec.PBEKeySpec;
import java.security.SecureRandom;
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
    private static final SecureRandom RANDOM = new SecureRandom();
    private final Map<String, User> users = new ConcurrentHashMap<>();
    private final Map<String, String> sessions = new ConcurrentHashMap<>();
    private final List<Message> messages = new CopyOnWriteArrayList<>(List.of(
            new Message("Maya", "The new workspace is looking sharp.", "09:41", false),
            new Message("Maya", "I left a few notes in the project channel.", "09:43", false)
    ));

    public static void main(String[] args) {
        SpringApplication.run(ChatServer.class, args);
    }

    @GetMapping("/health")
    public Map<String, String> health() {
        return Map.of("status", "ok");
    }

    @GetMapping("/api/auth/me")
    public UserResponse currentUser(HttpServletRequest request) {
        return userResponse(requireUser(request));
    }

    @PostMapping("/api/auth/signup")
    public UserResponse signup(@RequestBody(required = false) SignupRequest request,
                               HttpServletResponse response) {
        if (request == null || blank(request.name()) || blank(request.email()) || blank(request.password())) {
            throw badRequest("Name, email, and password are required");
        }
        if (request.password().length() < 6) {
            throw badRequest("Password must be at least 6 characters");
        }
        String email = request.email().trim().toLowerCase(Locale.ROOT);
        if (!email.contains("@")) {
            throw badRequest("Enter a valid email address");
        }
        User user = new User(request.name().trim(), email, hashPassword(request.password()));
        if (users.putIfAbsent(email, user) != null) {
            throw new ResponseStatusException(HttpStatus.CONFLICT, "An account with that email already exists");
        }
        createSession(user, response);
        return userResponse(user);
    }

    @PostMapping("/api/auth/login")
    public UserResponse login(@RequestBody(required = false) LoginRequest request,
                              HttpServletResponse response) {
        if (request == null || blank(request.email()) || blank(request.password())) {
            throw badRequest("Email and password are required");
        }
        User user = users.get(request.email().trim().toLowerCase(Locale.ROOT));
        if (user == null || !matchesPassword(request.password(), user.passwordHash())) {
            throw new ResponseStatusException(HttpStatus.UNAUTHORIZED, "Incorrect email or password");
        }
        createSession(user, response);
        return userResponse(user);
    }

    @PostMapping("/api/auth/logout")
    public void logout(HttpServletRequest request, HttpServletResponse response) {
        String token = cookieValue(request, SESSION_COOKIE);
        if (token != null) {
            sessions.remove(token);
        }
        Cookie cookie = new Cookie(SESSION_COOKIE, "");
        cookie.setMaxAge(0);
        cookie.setPath("/");
        response.addCookie(cookie);
    }

    @GetMapping("/api/messages")
    public List<Message> getMessages(HttpServletRequest request) {
        User user = requireUser(request);
        return messages.stream()
                .map(message -> new Message(message.name(), message.text(), message.time(), message.name().equals(user.name())))
                .toList();
    }

    @PostMapping("/api/messages")
    public Message addMessage(@RequestBody(required = false) MessageRequest request,
                              HttpServletRequest httpRequest) {
        User user = requireUser(httpRequest);
        if (request == null || blank(request.text())) {
            throw badRequest("Message text is required");
        }
        String safeText = request.text().trim();
        if (safeText.length() > 2000) {
            throw badRequest("Messages cannot exceed 2000 characters");
        }
        Message message = new Message(user.name(), safeText, LocalTime.now().format(TIME_FORMAT), true);
        messages.add(message);
        return message;
    }

    private User requireUser(HttpServletRequest request) {
        String token = cookieValue(request, SESSION_COOKIE);
        User user = token == null ? null : users.get(sessions.get(token));
        if (user == null) {
            throw new ResponseStatusException(HttpStatus.UNAUTHORIZED, "Please log in");
        }
        return user;
    }

    private void createSession(User user, HttpServletResponse response) {
        String token = UUID.randomUUID().toString();
        sessions.put(token, user.email());
        Cookie cookie = new Cookie(SESSION_COOKIE, token);
        cookie.setHttpOnly(true);
        cookie.setMaxAge(60 * 60 * 24 * 7);
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

    private static UserResponse userResponse(User user) {
        return new UserResponse(user.name(), user.email());
    }

    private static boolean blank(String value) {
        return value == null || value.isBlank();
    }

    private static ResponseStatusException badRequest(String message) {
        return new ResponseStatusException(HttpStatus.BAD_REQUEST, message);
    }

    private static String hashPassword(String password) {
        try {
            byte[] salt = new byte[16];
            RANDOM.nextBytes(salt);
            byte[] hash = derive(password, salt);
            return Base64.getEncoder().encodeToString(salt) + ":" + Base64.getEncoder().encodeToString(hash);
        } catch (Exception exception) {
            throw new IllegalStateException("Unable to secure password", exception);
        }
    }

    private static boolean matchesPassword(String password, String stored) {
        try {
            String[] parts = stored.split(":", 2);
            return parts.length == 2 && Arrays.equals(
                    derive(password, Base64.getDecoder().decode(parts[0])),
                    Base64.getDecoder().decode(parts[1]));
        } catch (Exception exception) {
            return false;
        }
    }

    private static byte[] derive(String password, byte[] salt) throws Exception {
        PBEKeySpec spec = new PBEKeySpec(password.toCharArray(), salt, 120_000, 256);
        return SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).getEncoded();
    }

    private record User(String name, String email, String passwordHash) {}
    public record SignupRequest(String name, String email, String password) {}
    public record LoginRequest(String email, String password) {}
    public record UserResponse(String name, String email) {}
    public record MessageRequest(String text) {}
    public record Message(String name, String text, String time, boolean mine) {}
}
