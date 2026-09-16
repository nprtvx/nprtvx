FROM node:22-alpine AS frontend

WORKDIR /frontend
COPY frontend/package.json frontend/angular.json frontend/tsconfig.json frontend/tsconfig.app.json ./
RUN npm install --no-audit --no-fund
COPY frontend/src ./src
RUN npm run build

FROM maven:3.9-eclipse-temurin-17 AS build

WORKDIR /app
COPY pom.xml .
COPY server ./server
COPY --from=frontend /frontend/dist/neonmonkey/browser ./server/src/main/resources/static
RUN mvn -q clean package -DskipTests

FROM eclipse-temurin:17-jre

WORKDIR /app
COPY --from=build /app/target/neonmonkey-1.0.0.jar app.jar

EXPOSE 8080

CMD ["sh", "-c", "exec java ${JAVA_OPTS:-} -Dserver.port=${PORT:-8080} -jar app.jar"]
