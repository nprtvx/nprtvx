from flask import Flask
import requests
from src.home import home
from src.popeye import popeye
from src.account.users import username

from fastapi import FastAPI
from flask import Flask
from werkzeug.middleware.dispatcher import DispatcherMiddleware
from a2wsgi import ASGIMiddleware

flask_app = Flask(__name__)

@flask_app.route("/")
def home():
    return "Hello from Flask!"

fastapi_app = FastAPI()

@fastapi_app.get("/")
async def hello_fastapi():
    return {"msg": "Hello from FastAPI"}

# Mount FastAPI inside Flask at the "/fastapi" path
flask_app.wsgi_app = DispatcherMiddleware(
    flask_app.wsgi_app, {
        '/fastapi': ASGIMiddleware(fastapi_app)
    }
)

if __name__ == "__main__":
    flask_app.run()


#eof