from fastapi import FastAPI
from flask import Flask
from werkzeug.middleware.dispatcher import DispatcherMiddleware
from a2wsgi import ASGIMiddleware

app = Flask(__name__)

@app.route("/")
def home():
    return "Hello from Flask!"

fastapi_app = FastAPI()

@fastapi_app.get("/")
async def hello_fastapi():
    return {"msg": "Hello from FastAPI"}

# Mount FastAPI inside Flask at the "/fastapi" path
app.wsgi_app = DispatcherMiddleware(
    app.wsgi_app, {
        '/fastapi': ASGIMiddleware(fastapi_app)
    }
)

if __name__ == "__main__":
    app.run()


#eof