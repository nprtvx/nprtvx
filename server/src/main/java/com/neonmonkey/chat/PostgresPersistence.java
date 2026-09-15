package com.neonmonkey.chat;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.springframework.core.io.ClassPathResource;
import org.springframework.stereotype.Component;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.sql.*;
import java.util.*;

@Component
public final class PostgresPersistence {
    private final boolean enabled;
    private final String jdbcUrl;
    private final ObjectMapper objectMapper = new ObjectMapper();

    public PostgresPersistence() {
        String configuredUrl = firstNonBlank(System.getenv("JDBC_DATABASE_URL"), System.getenv("DATABASE_URL"));
        if (configuredUrl == null) {
            enabled = false;
            jdbcUrl = null;
            return;
        }
        enabled = true;
        jdbcUrl = normalizeUrl(configuredUrl);
        initializeSchema();
    }

    public Snapshot load() {
        if (!enabled) return Snapshot.empty();
        try (Connection connection = connection()) {
            Map<String, ChatServer.Identity> identities = loadIdentities(connection);
            Map<String, String> sessions = loadSessions(connection);
            List<ChatServer.EncryptedMessage> messages = loadDirectMessages(connection);
            List<ChatServer.EncryptedAttachment> attachments = loadAttachments(connection);
            Map<String, ChatServer.Group> groups = loadGroups(connection);
            List<ChatServer.EncryptedGroupMessage> groupMessages = loadGroupMessages(connection);
            return new Snapshot(identities, sessions, messages, attachments, groups, groupMessages);
        } catch (SQLException exception) {
            throw databaseFailure("Could not load persisted chat data", exception);
        }
    }

    public void saveIdentity(ChatServer.Identity identity) {
        execute("""
                INSERT INTO identities (account_id, display_name, public_key, encrypted_recovery_bundle)
                VALUES (?, ?, ?::jsonb, ?::jsonb)
                ON CONFLICT (account_id) DO UPDATE SET
                    public_key = EXCLUDED.public_key,
                    encrypted_recovery_bundle = EXCLUDED.encrypted_recovery_bundle,
                    last_seen_at = CURRENT_TIMESTAMP
                """, identity.accountId(), identity.displayName(), json(identity.publicKey()), json(identity.recoveryBundle()));
    }

    public void saveSession(String token, String accountId, long expiresAt) {
        execute("""
                INSERT INTO sessions (session_id, account_id, expires_at)
                VALUES (?::uuid, ?, to_timestamp(CAST(? AS double precision) / 1000.0))
                ON CONFLICT (session_id) DO UPDATE SET account_id = EXCLUDED.account_id,
                    expires_at = EXCLUDED.expires_at
                """, token, accountId, expiresAt);
    }

    public void deleteSession(String token) {
        execute("DELETE FROM sessions WHERE session_id = ?::uuid", token);
    }

    public void saveDirectMessage(ChatServer.EncryptedMessage message) {
        execute("""
                INSERT INTO encrypted_messages
                    (message_id, conversation_id, sender_account_id, recipient_account_id, ciphertext, expires_at, created_at)
                VALUES (?::uuid, ?::uuid, ?, ?, jsonb_build_object('iv', ?, 'ciphertext', ?),
                        to_timestamp(CAST(? AS double precision) / 1000.0),
                        to_timestamp(CAST(? AS double precision) / 1000.0))
                """, UUID.randomUUID(), conversationId(message.senderAccountId(), message.recipientAccountId()),
                message.senderAccountId(), message.recipientAccountId(), message.iv(), message.ciphertext(),
                message.expiresAt(), message.createdAt());
    }

    public void saveGroup(ChatServer.Group group) {
        try (Connection connection = connection()) {
            connection.setAutoCommit(false);
            try (PreparedStatement groupStatement = connection.prepareStatement("""
                    INSERT INTO groups (group_id, name, owner_account_id)
                    VALUES (?::uuid, ?, ?)
                    ON CONFLICT (group_id) DO UPDATE SET name = EXCLUDED.name,
                        owner_account_id = EXCLUDED.owner_account_id
                    """);
                 PreparedStatement memberStatement = connection.prepareStatement("""
                    INSERT INTO group_members (group_id, account_id, encrypted_group_key)
                    VALUES (?::uuid, ?, ?::jsonb)
                    ON CONFLICT (group_id, account_id) DO UPDATE SET
                        encrypted_group_key = EXCLUDED.encrypted_group_key
                    """)) {
                groupStatement.setString(1, group.groupId());
                groupStatement.setString(2, group.name());
                groupStatement.setString(3, group.ownerAccountId());
                groupStatement.executeUpdate();
                for (String member : group.members()) {
                    memberStatement.setString(1, group.groupId());
                    memberStatement.setString(2, member);
                    memberStatement.setString(3, json(group.memberKeys().get(member)));
                    memberStatement.addBatch();
                }
                memberStatement.executeBatch();
                connection.commit();
            } catch (SQLException exception) {
                connection.rollback();
                throw exception;
            }
        } catch (SQLException exception) {
            throw databaseFailure("Could not persist group", exception);
        }
    }

    public void saveGroupMessage(ChatServer.EncryptedGroupMessage message) {
        execute("""
                INSERT INTO encrypted_group_messages
                    (message_id, group_id, sender_account_id, ciphertext, expires_at, created_at)
                VALUES (?::uuid, ?::uuid, ?, jsonb_build_object('iv', ?, 'ciphertext', ?),
                        to_timestamp(CAST(? AS double precision) / 1000.0),
                        to_timestamp(CAST(? AS double precision) / 1000.0))
                """, UUID.randomUUID(), message.groupId(), message.senderAccountId(), message.iv(),
                message.ciphertext(), message.expiresAt(), message.createdAt());
    }

    public void saveAttachment(ChatServer.EncryptedAttachment attachment) {
        execute("""
                INSERT INTO encrypted_attachments
                    (attachment_id, sender_account_id, recipient_account_id, group_id, name, mime_type,
                     ciphertext, expires_at, created_at)
                VALUES (?::uuid, ?, ?, ?::uuid, ?, ?, jsonb_build_object('iv', ?, 'ciphertext', ?),
                        to_timestamp(CAST(? AS double precision) / 1000.0),
                        to_timestamp(CAST(? AS double precision) / 1000.0))
                """, attachment.attachmentId(), attachment.senderAccountId(), attachment.recipientAccountId(),
                attachment.groupId(), attachment.name(), attachment.mimeType(), attachment.iv(),
                attachment.ciphertext(), attachment.expiresAt(), attachment.createdAt());
    }

    public void deleteExpired(long now) {
        if (!enabled) return;
        execute("DELETE FROM encrypted_messages WHERE expires_at IS NOT NULL AND expires_at <= to_timestamp(CAST(? AS double precision) / 1000.0)", now);
        execute("DELETE FROM encrypted_group_messages WHERE expires_at IS NOT NULL AND expires_at <= to_timestamp(CAST(? AS double precision) / 1000.0)", now);
        execute("DELETE FROM encrypted_attachments WHERE expires_at IS NOT NULL AND expires_at <= to_timestamp(CAST(? AS double precision) / 1000.0)", now);
        execute("DELETE FROM sessions WHERE expires_at <= to_timestamp(CAST(? AS double precision) / 1000.0)", now);
    }

    private Map<String, ChatServer.Identity> loadIdentities(Connection connection) throws SQLException {
        Map<String, ChatServer.Identity> result = new LinkedHashMap<>();
        try (PreparedStatement statement = connection.prepareStatement("""
                SELECT trim(account_id), display_name, public_key::text, encrypted_recovery_bundle::text
                FROM identities
                """)) {
            try (ResultSet rows = statement.executeQuery()) {
                while (rows.next()) {
                    result.put(rows.getString(1), new ChatServer.Identity(rows.getString(1), rows.getString(2),
                            readJsonString(rows.getString(3)), readJsonString(rows.getString(4))));
                }
            }
        }
        return result;
    }

    private Map<String, String> loadSessions(Connection connection) throws SQLException {
        Map<String, String> result = new LinkedHashMap<>();
        try (PreparedStatement statement = connection.prepareStatement("""
                SELECT session_id::text, trim(account_id)
                FROM sessions
                WHERE expires_at > CURRENT_TIMESTAMP
                """);
             ResultSet rows = statement.executeQuery()) {
            while (rows.next()) result.put(rows.getString(1), rows.getString(2));
        }
        return result;
    }

    private List<ChatServer.EncryptedMessage> loadDirectMessages(Connection connection) throws SQLException {
        List<ChatServer.EncryptedMessage> result = new ArrayList<>();
        try (PreparedStatement statement = connection.prepareStatement("""
                SELECT trim(sender_account_id), trim(recipient_account_id), ciphertext->>'iv',
                       ciphertext->>'ciphertext',
                       (extract(epoch FROM created_at) * 1000)::bigint,
                       CASE WHEN expires_at IS NULL THEN NULL
                            ELSE (extract(epoch FROM expires_at) * 1000)::bigint END
                FROM encrypted_messages ORDER BY created_at
                """);
             ResultSet rows = statement.executeQuery()) {
            while (rows.next()) result.add(new ChatServer.EncryptedMessage(rows.getString(1), rows.getString(2),
                    rows.getString(3), rows.getString(4), rows.getLong(5), nullableLong(rows, 6)));
        }
        return result;
    }

    private Map<String, ChatServer.Group> loadGroups(Connection connection) throws SQLException {
        Map<String, Set<String>> members = new LinkedHashMap<>();
        Map<String, Map<String, String>> memberKeys = new LinkedHashMap<>();
        try (PreparedStatement statement = connection.prepareStatement("""
                SELECT group_id::text, trim(account_id), encrypted_group_key::text
                FROM group_members ORDER BY group_id
                """);
             ResultSet rows = statement.executeQuery()) {
            while (rows.next()) {
                members.computeIfAbsent(rows.getString(1), ignored -> new LinkedHashSet<>()).add(rows.getString(2));
                memberKeys.computeIfAbsent(rows.getString(1), ignored -> new LinkedHashMap<>())
                        .put(rows.getString(2), readJsonString(rows.getString(3)));
            }
        }
        Map<String, ChatServer.Group> result = new LinkedHashMap<>();
        try (PreparedStatement statement = connection.prepareStatement("""
                SELECT group_id::text, name, trim(owner_account_id) FROM groups ORDER BY created_at
                """);
             ResultSet rows = statement.executeQuery()) {
            while (rows.next()) {
                String groupId = rows.getString(1);
                result.put(groupId, new ChatServer.Group(groupId, rows.getString(2), rows.getString(3),
                        members.getOrDefault(groupId, Set.of()),
                        memberKeys.getOrDefault(groupId, Map.of())));
            }
        }
        return result;
    }

    private List<ChatServer.EncryptedGroupMessage> loadGroupMessages(Connection connection) throws SQLException {
        List<ChatServer.EncryptedGroupMessage> result = new ArrayList<>();
        try (PreparedStatement statement = connection.prepareStatement("""
                SELECT group_id::text, trim(sender_account_id), ciphertext->>'iv',
                       ciphertext->>'ciphertext',
                       (extract(epoch FROM created_at) * 1000)::bigint,
                       CASE WHEN expires_at IS NULL THEN NULL
                            ELSE (extract(epoch FROM expires_at) * 1000)::bigint END
                FROM encrypted_group_messages ORDER BY created_at
                """);
             ResultSet rows = statement.executeQuery()) {
            while (rows.next()) result.add(new ChatServer.EncryptedGroupMessage(rows.getString(1), rows.getString(2),
                    rows.getString(3), rows.getString(4), rows.getLong(5), nullableLong(rows, 6)));
        }
        return result;
    }

    private List<ChatServer.EncryptedAttachment> loadAttachments(Connection connection) throws SQLException {
        List<ChatServer.EncryptedAttachment> result = new ArrayList<>();
        try (PreparedStatement statement = connection.prepareStatement("""
                SELECT attachment_id::text, trim(sender_account_id), trim(recipient_account_id), group_id::text,
                       name, mime_type, ciphertext->>'iv', ciphertext->>'ciphertext',
                       (extract(epoch FROM created_at) * 1000)::bigint,
                       CASE WHEN expires_at IS NULL THEN NULL
                            ELSE (extract(epoch FROM expires_at) * 1000)::bigint END
                FROM encrypted_attachments ORDER BY created_at
                """);
             ResultSet rows = statement.executeQuery()) {
            while (rows.next()) {
                result.add(new ChatServer.EncryptedAttachment(rows.getString(1), rows.getString(2),
                        nullableString(rows, 3), nullableString(rows, 4), rows.getString(5), rows.getString(6),
                        rows.getString(7), rows.getString(8), rows.getLong(9), nullableLong(rows, 10)));
            }
        }
        return result;
    }

    private void initializeSchema() {
        try {
            String schema = new String(new ClassPathResource("db/schema.sql").getInputStream().readAllBytes(),
                    StandardCharsets.UTF_8);
            try (Connection connection = connection(); Statement statement = connection.createStatement()) {
                for (String sql : schema.split(";")) {
                    String command = sql.trim();
                    if (!command.isEmpty()) statement.execute(command);
                }
            }
            ensureIdentityDisplayNameColumn(null);
        } catch (IOException | SQLException exception) {
            throw databaseFailure("Could not initialize PostgreSQL schema", exception);
        }
    }

    private void ensureIdentityDisplayNameColumn(Connection ignored) throws SQLException {
        execute("ALTER TABLE identities ADD COLUMN IF NOT EXISTS display_name TEXT NOT NULL DEFAULT ''");
    }

    private void execute(String sql, Object... values) {
        if (!enabled) return;
        try (Connection connection = connection(); PreparedStatement statement = connection.prepareStatement(sql)) {
            for (int index = 0; index < values.length; index++) statement.setObject(index + 1, values[index]);
            statement.executeUpdate();
        } catch (SQLException exception) {
            throw databaseFailure("Could not persist chat data", exception);
        }
    }

    private Connection connection() throws SQLException {
        return DriverManager.getConnection(jdbcUrl);
    }

    private String json(String value) {
        try {
            return objectMapper.writeValueAsString(value);
        } catch (JsonProcessingException exception) {
            throw new IllegalStateException("Could not encode persisted value", exception);
        }
    }

    private String readJsonString(String value) {
        try {
            return objectMapper.readValue(value, String.class);
        } catch (Exception ignored) {
            return value;
        }
    }

    private static Long nullableLong(ResultSet rows, int index) throws SQLException {
        long value = rows.getLong(index);
        return rows.wasNull() ? null : value;
    }

    private static String nullableString(ResultSet rows, int index) throws SQLException {
        return rows.getString(index);
    }

    private static UUID conversationId(String first, String second) {
        String value = first.compareTo(second) < 0 ? first + ":" + second : second + ":" + first;
        return UUID.nameUUIDFromBytes(value.getBytes(StandardCharsets.UTF_8));
    }

    private static String normalizeUrl(String url) {
        if (url.startsWith("postgres://")) return "jdbc:postgresql://" + url.substring("postgres://".length());
        if (url.startsWith("postgresql://")) return "jdbc:postgresql://" + url.substring("postgresql://".length());
        return url;
    }

    private static String firstNonBlank(String first, String second) {
        return first != null && !first.isBlank() ? first : (second != null && !second.isBlank() ? second : null);
    }

    private static IllegalStateException databaseFailure(String message, Exception cause) {
        return new IllegalStateException(message, cause);
    }

    public record Snapshot(Map<String, ChatServer.Identity> identities, Map<String, String> sessions,
                           List<ChatServer.EncryptedMessage> messages,
                           List<ChatServer.EncryptedAttachment> attachments,
                           Map<String, ChatServer.Group> groups,
                           List<ChatServer.EncryptedGroupMessage> groupMessages) {
        static Snapshot empty() {
            return new Snapshot(Map.of(), Map.of(), List.of(), List.of(), Map.of(), List.of());
        }
    }
}
