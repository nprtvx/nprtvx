package com.neonmonkey.chat;

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;

import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.time.Instant;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Executors;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

public final class ChatServer {
    private static final int PORT = 8080;
    private static final List<Message> messages = new ArrayList<>(List.of(
            new Message("Maya", "The new workspace is looking sharp.", "09:41", false),
            new Message("You", "That was the goal. Welcome in!", "09:42", true),
            new Message("Maya", "I left a few notes in the project channel.", "09:43", false)
    ));
    private static final Pattern JSON_FIELD = Pattern.compile("\\\"(name|text)\\\"\\s*:\\s*\\\"((?:\\\\.|[^\\\"])*)\\\"");

    public static void main(String[] args) throws IOException {
        HttpServer server = HttpServer.create(new InetSocketAddress(PORT), 0);
        server.createContext("/api/messages", ChatServer::handleMessages);
        server.createContext("/", ChatServer::serveStatic);
        server.setExecutor(Executors.newFixedThreadPool(4));
        server.start();
        System.out.println("Chat server running at http://localhost:" + PORT);
    }

    private static void handleMessages(HttpExchange exchange) throws IOException {
        addCorsHeaders(exchange);
        if ("OPTIONS".equals(exchange.getRequestMethod())) {
            send(exchange, 204, "");
            return;
        }
        if ("GET".equals(exchange.getRequestMethod())) {
            synchronized (messages) {
                StringBuilder json = new StringBuilder("[");
                for (int i = 0; i < messages.size(); i++) {
                    if (i > 0) json.append(',');
                    json.append(messages.get(i).toJson());
                }
                send(exchange, 200, json.append(']').toString(), "application/json");
            }
            return;
        }
        if ("POST".equals(exchange.getRequestMethod())) {
            String body = new String(exchange.getRequestBody().readAllBytes(), StandardCharsets.UTF_8);
            Matcher matcher = JSON_FIELD.matcher(body);
            String name = null;
            String text = null;
            while (matcher.find()) {
                if ("name".equals(matcher.group(1))) name = unescape(matcher.group(2));
                if ("text".equals(matcher.group(1))) text = unescape(matcher.group(2));
            }
            if (text == null || text.isBlank()) {
                send(exchange, 400, "{\"error\":\"Message text is required\"}", "application/json");
                return;
            }
            Message message = new Message(name == null || name.isBlank() ? "You" : name, text.trim(), "now", true);
            synchronized (messages) { messages.add(message); }
            send(exchange, 201, message.toJson(), "application/json");
            return;
        }
        send(exchange, 405, "{\"error\":\"Method not allowed\"}", "application/json");
    }

    private static void serveStatic(HttpExchange exchange) throws IOException {
        String requestPath = exchange.getRequestURI().getPath();
        if (requestPath.equals("/")) requestPath = "/index.html";
        Path root = Paths.get("server", "src", "main", "resources", "static").toAbsolutePath().normalize();
        Path file = root.resolve(requestPath.substring(1)).normalize();
        if (!file.startsWith(root) || !Files.isRegularFile(file)) {
            send(exchange, 404, "Not found");
            return;
        }
        String contentType = Map.of(".html", "text/html; charset=utf-8", ".css", "text/css; charset=utf-8", ".js", "application/javascript; charset=utf-8").getOrDefault(extension(file), "application/octet-stream");
        exchange.getResponseHeaders().set("Content-Type", contentType);
        byte[] content = Files.readAllBytes(file);
        exchange.sendResponseHeaders(200, content.length);
        try (OutputStream output = exchange.getResponseBody()) { output.write(content); }
    }

    private static String extension(Path path) {
        String name = path.getFileName().toString();
        int dot = name.lastIndexOf('.');
        return dot < 0 ? "" : name.substring(dot);
    }

    private static void addCorsHeaders(HttpExchange exchange) {
        exchange.getResponseHeaders().set("Access-Control-Allow-Origin", "*");
        exchange.getResponseHeaders().set("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
        exchange.getResponseHeaders().set("Access-Control-Allow-Headers", "Content-Type");
    }

    private static void send(HttpExchange exchange, int status, String body) throws IOException { send(exchange, status, body, "text/plain; charset=utf-8"); }

    private static void send(HttpExchange exchange, int status, String body, String contentType) throws IOException {
        exchange.getResponseHeaders().set("Content-Type", contentType);
        byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        exchange.sendResponseHeaders(status, bytes.length);
        try (OutputStream output = exchange.getResponseBody()) { output.write(bytes); }
    }

    private static String unescape(String value) { return value.replace("\\\"", "\"").replace("\\\\", "\\"); }

    private record Message(String name, String text, String time, boolean mine) {
        String toJson() { return "{\"name\":\"" + escape(name) + "\",\"text\":\"" + escape(text) + "\",\"time\":\"" + time + "\",\"mine\":" + mine + "}"; }
        private static String escape(String value) { return value.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n"); }
    }
}
