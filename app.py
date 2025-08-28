from fastapi import FastAPI
from flask import Flask, render_template, abort
from werkzeug.middleware.dispatcher import DispatcherMiddleware
from a2wsgi import ASGIMiddleware
import os
from src.home import home

def create_page(page_name: str | None = None):
  if page_name is None:
    page_name="home"
  if page_name:
    with open(f"templates/{page_name}.html", 'w') as page:
      page.write(home.toString())
      page.close()

create_page("home")
app = Flask(__name__, template_folder="templates")

@app.route("/")
def index():
    # Ensure template exists
    template_path = os.path.join(app.template_folder, "home.html")
    if not os.path.exists(template_path):
        return "<h1>Home Page Not Found</h1>", 404
    return home, 200

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
