# Gather Chat

A small Java web server for a polished static team chat interface. Messages are stored in memory for the life of the server.

## Run

Requires JDK 17 or newer.

```powershell
javac -d out server/src/main/java/com/neonmonkey/chat/ChatServer.java
java -cp out com.neonmonkey.chat.ChatServer
```

Open http://localhost:8080.
