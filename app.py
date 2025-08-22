from fastapi import FastAPI
from flask import Flask, render_template, abort
from werkzeug.middleware.dispatcher import DispatcherMiddleware
from a2wsgi import ASGIMiddleware
import os

app = Flask(__name__, template_folder="templates")

@app.route("/")
def home():
    # Ensure template exists
    template_path = os.path.join(app.template_folder, "home.html")
    if not os.path.exists(template_path):
        return "<h1>Home Page Not Found</h1>", 404
    return render_template("src/home.html")

# FastAPI app for additional APIs
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
