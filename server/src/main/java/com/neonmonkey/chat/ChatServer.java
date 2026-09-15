package com.neonmonkey.chat;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

@SpringBootApplication
@RestController
@RequestMapping("/api/messages")
public final class ChatServer {
    private static final DateTimeFormatter TIME_FORMAT = DateTimeFormatter.ofPattern("HH:mm");
    private final List<Message> messages = new CopyOnWriteArrayList<>(List.of(
            new Message("Maya", "The new workspace is looking sharp.", "09:41", false),
            new Message("You", "That was the goal. Welcome in!", "09:42", true),
            new Message("Maya", "I left a few notes in the project channel.", "09:43", false)
    ));

    public static void main(String[] args) {
        SpringApplication.run(ChatServer.class, args);
    }

    @GetMapping
    public List<Message> getMessages() {
        return List.copyOf(messages);
    }

    @PostMapping
    public Message addMessage(@RequestBody(required = false) MessageRequest request) {
        if (request == null || request.text() == null || request.text().isBlank()) {
            throw new ResponseStatusException(HttpStatus.BAD_REQUEST, "Message text is required");
        }

        String safeName = request.name() == null ? "You" : request.name().trim();
        if (safeName.isBlank()) {
            safeName = "You";
        }

        String safeText = request.text().trim();
        Message message = new Message(
                safeName,
                safeText,
                LocalTime.now().format(TIME_FORMAT),
                true
        );
        messages.add(message);
        return message;
    }

    public record MessageRequest(String name, String text) {
    }

    public record Message(String name, String text, String time, boolean mine) {
    }
}
