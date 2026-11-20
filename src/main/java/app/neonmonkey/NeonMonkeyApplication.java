package app.neonmonkey;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Base64;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.UUID;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.context.annotation.Bean;
import org.springframework.http.HttpStatus;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.scheduling.annotation.EnableScheduling;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.security.authentication.UsernamePasswordAuthenticationToken;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.config.http.SessionCreationPolicy;
import org.springframework.security.core.context.SecurityContextHolder;
import org.springframework.security.crypto.bcrypt.BCryptPasswordEncoder;
import org.springframework.security.crypto.password.PasswordEncoder;
import org.springframework.security.web.SecurityFilterChain;
import org.springframework.security.web.authentication.UsernamePasswordAuthenticationFilter;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.filter.OncePerRequestFilter;
import org.springframework.web.server.ResponseStatusException;

import jakarta.servlet.FilterChain;
import jakarta.servlet.ServletException;
import jakarta.servlet.http.HttpServletRequest;
import jakarta.servlet.http.HttpServletResponse;

@SpringBootApplication
@EnableScheduling
public class NeonMonkeyApplication {
    public static void main(String[] args) { SpringApplication.run(NeonMonkeyApplication.class, args); }

    @Bean PasswordEncoder passwordEncoder() { return new BCryptPasswordEncoder(12); }

    @Bean SecurityFilterChain security(HttpSecurity http, BearerAuthFilter authFilter) throws Exception {
        return http.csrf(csrf -> csrf.disable())
                .sessionManagement(sm -> sm.sessionCreationPolicy(SessionCreationPolicy.STATELESS))
                .authorizeHttpRequests(auth -> auth.requestMatchers("/health", "/api/auth/**").permitAll().anyRequest().authenticated())
                .addFilterBefore(authFilter, UsernamePasswordAuthenticationFilter.class)
                .build();
    }
}

@Component
class BearerAuthFilter extends OncePerRequestFilter {
    private final JdbcTemplate db;
    BearerAuthFilter(JdbcTemplate db) { this.db = db; }
    @Override protected void doFilterInternal(HttpServletRequest request, HttpServletResponse response, FilterChain chain)
            throws ServletException, java.io.IOException {
        String header = request.getHeader("Authorization");
        if (header != null && header.startsWith("Bearer ")) {
            try {
                String tokenHash = ChatApi.sha256(header.substring(7));
                List<String> users = db.query("SELECT u.username FROM auth_sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=? AND s.expires_at > now()", (rs, row) -> rs.getString(1), tokenHash);
                if (!users.isEmpty()) SecurityContextHolder.getContext().setAuthentication(
                        new UsernamePasswordAuthenticationToken(users.get(0), null, List.of()));
            } catch (RuntimeException ignored) { SecurityContextHolder.clearContext(); }
        }
        chain.doFilter(request, response);
    }
}

@RestController
class ChatApi {
    private static final SecureRandom RANDOM = new SecureRandom();
    private final JdbcTemplate db;
    private final PasswordEncoder passwords;
    ChatApi(JdbcTemplate db, PasswordEncoder passwords) { this.db = db; this.passwords = passwords; }

    record Credentials(String username, String passphrase) {}
    record AuthResult(String username, String accessToken, Instant expiresAt) {}
    record NewRoom(String kind, String title, List<String> participants) {}
    record RoomView(UUID id, String kind, String title, List<String> participants, Instant createdAt) {}
    record NewMessage(String ciphertext, Integer expiresInSeconds) {}
    record MessageView(UUID id, String sender, String ciphertext, Instant createdAt, Instant expiresAt) {}

    @GetMapping("/health") Map<String, String> health() { return Map.of("status", "ok"); }

    @PostMapping("/api/auth/register")
    @Transactional
    AuthResult register(@RequestBody Credentials input) {
        String username = normalizeUsername(input.username());
        String passphrase = input.passphrase();
        if (passphrase == null || passphrase.length() < 12 || passphrase.length() > 200)
            throw bad("Passphrase must be between 12 and 200 characters");
        if (!db.query("SELECT id FROM users WHERE username=?", (rs, row) -> rs.getString(1), username).isEmpty())
            throw new ResponseStatusException(HttpStatus.CONFLICT, "Username is unavailable");
        UUID id = UUID.randomUUID();
        db.update("INSERT INTO users(id, username, password_hash) VALUES (?, ?, ?)", id, username, passwords.encode(passphrase));
        return issueSession(id, username);
    }

    @PostMapping("/api/auth/login")
    @Transactional
    AuthResult login(@RequestBody Credentials input) {
        String username = normalizeUsername(input.username());
        List<Map<String, Object>> rows = db.queryForList("SELECT id, password_hash FROM users WHERE username=?", username);
        if (rows.isEmpty() || input.passphrase() == null || !passwords.matches(input.passphrase(), (String) rows.get(0).get("password_hash")))
            throw new ResponseStatusException(HttpStatus.UNAUTHORIZED, "Invalid username or passphrase");
        return issueSession((UUID) rows.get(0).get("id"), username);
    }

    private AuthResult issueSession(UUID userId, String username) {
        byte[] raw = new byte[32]; RANDOM.nextBytes(raw);
        String token = Base64.getUrlEncoder().withoutPadding().encodeToString(raw);
        Instant expires = Instant.now().plusSeconds(30L * 24 * 60 * 60);
        db.update("INSERT INTO auth_sessions(token_hash,user_id,expires_at) VALUES (?,?,?)", sha256(token), userId, expires);
        return new AuthResult(username, token, expires);
    }

    @PostMapping("/api/rooms")
    @Transactional
    RoomView createRoom(@RequestBody NewRoom input, java.security.Principal principal) {
        String kind = input.kind() == null ? "" : input.kind().toLowerCase();
        if (!kind.equals("direct") && !kind.equals("group")) throw bad("kind must be direct or group");
        List<String> names = new ArrayList<>(); names.add(principal.getName());
        if (input.participants() != null) input.participants().stream().map(ChatApi::normalizeUsername).filter(n -> !names.contains(n)).forEach(names::add);
        if (names.size() < 2 || names.size() > 100 || (kind.equals("direct") && names.size() != 2))
            throw bad("A direct chat must have 2 participants; a group must have 2-100");
        String title = input.title() == null ? null : input.title().trim();
        if (kind.equals("group") && (title == null || title.isBlank() || title.length() > 100)) throw bad("Group title is required (max 100 characters)");
        for (String name : names) if (db.queryForObject("SELECT count(*) FROM users WHERE username=?", Integer.class, name) == 0)
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, "Unknown participant: " + name);
        UUID roomId = UUID.randomUUID();
        db.update("INSERT INTO chat_rooms(id,kind,title) VALUES (?,?,?)", roomId, kind, title);
        for (String name : names) db.update("INSERT INTO room_members(room_id,user_id) SELECT ?,id FROM users WHERE username=?", roomId, name);
        return room(roomId);
    }

    @GetMapping("/api/rooms")
    List<RoomView> rooms(java.security.Principal principal) {
        return db.query("SELECT r.id FROM chat_rooms r JOIN room_members m ON m.room_id=r.id JOIN users u ON u.id=m.user_id WHERE u.username=? ORDER BY r.created_at DESC",
                (rs, row) -> room((UUID) rs.getObject(1)), principal.getName());
    }

    @PostMapping("/api/rooms/{roomId}/messages")
    @Transactional
    MessageView send(@PathVariable UUID roomId, @RequestBody NewMessage input, java.security.Principal principal) {
        requireMember(roomId, principal.getName());
        if (input.ciphertext() == null) throw bad("ciphertext is required");
        byte[] encrypted;
        try { encrypted = Base64.getDecoder().decode(input.ciphertext()); }
        catch (IllegalArgumentException ex) { throw bad("ciphertext must be base64"); }
        if (encrypted.length < 16 || encrypted.length > 12_000) throw bad("ciphertext must decode to 16-12000 bytes");
        Integer ttl = input.expiresInSeconds();
        if (ttl != null && (ttl < 1 || ttl > 2_592_000)) throw bad("expiry must be between 1 second and 30 days");
        UUID id = UUID.randomUUID();
        Instant created = Instant.now(); Instant expires = ttl == null ? null : created.plusSeconds(ttl);
        db.update("INSERT INTO messages(id,room_id,sender_id,ciphertext,created_at,expires_at) SELECT ?,?,?,?, ?,?",
                id, roomId, userId(principal.getName()), input.ciphertext(), created, expires);
        return new MessageView(id, principal.getName(), input.ciphertext(), created, expires);
    }

    @GetMapping("/api/rooms/{roomId}/messages")
    List<MessageView> messages(@PathVariable UUID roomId, java.security.Principal principal,
                               @RequestParam(defaultValue = "50") int limit) {
        requireMember(roomId, principal.getName());
        if (limit < 1 || limit > 100) throw bad("limit must be between 1 and 100");
        return db.query("SELECT m.id,u.username,m.ciphertext,m.created_at,m.expires_at FROM messages m JOIN users u ON u.id=m.sender_id WHERE m.room_id=? AND (m.expires_at IS NULL OR m.expires_at>now()) ORDER BY m.created_at DESC LIMIT ?",
                (rs, row) -> new MessageView((UUID) rs.getObject(1), rs.getString(2), rs.getString(3), rs.getTimestamp(4).toInstant(), rs.getTimestamp(5) == null ? null : rs.getTimestamp(5).toInstant()), roomId, limit);
    }

    private RoomView room(UUID id) {
        Map<String,Object> r = db.queryForMap("SELECT id,kind,title,created_at FROM chat_rooms WHERE id=?", id);
        List<String> members = db.query("SELECT u.username FROM users u JOIN room_members m ON m.user_id=u.id WHERE m.room_id=? ORDER BY u.username", (rs,row)->rs.getString(1), id);
        return new RoomView((UUID)r.get("id"),(String)r.get("kind"),(String)r.get("title"),members,((java.sql.Timestamp)r.get("created_at")).toInstant());
    }
    private void requireMember(UUID roomId, String username) {
        Integer count = db.queryForObject("SELECT count(*) FROM room_members m JOIN users u ON u.id=m.user_id WHERE m.room_id=? AND u.username=?", Integer.class, roomId, username);
        if (count == null || count == 0) throw new ResponseStatusException(HttpStatus.NOT_FOUND, "Chat not found");
    }
    private UUID userId(String username) { return db.queryForObject("SELECT id FROM users WHERE username=?", UUID.class, username); }
    private static String normalizeUsername(String value) {
        if (value == null) throw bad("username is required");
        String name = value.toLowerCase();
        if (!name.matches("[a-z0-9_]{3,24}")) throw bad("username must be 3-24 lowercase letters, digits, or underscores");
        return name;
    }
    static String sha256(String value) {
        try { return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(value.getBytes(StandardCharsets.UTF_8))); }
        catch (Exception ex) { throw new IllegalStateException(ex); }
    }
    private static ResponseStatusException bad(String message) { return new ResponseStatusException(HttpStatus.BAD_REQUEST, message); }

    @Scheduled(fixedDelay = 60_000)
    void deleteExpiredMessages() { db.update("DELETE FROM messages WHERE expires_at IS NOT NULL AND expires_at <= now()"); }
    @Scheduled(fixedDelay = 3_600_000)
    void deleteExpiredSessions() { db.update("DELETE FROM auth_sessions WHERE expires_at <= now()"); }
}
