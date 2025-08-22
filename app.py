from fastapi import FastAPI
from flask import Flask, render_template, abort
from werkzeug.middleware.dispatcher import DispatcherMiddleware
from a2wsgi import ASGIMiddleware
import os
from src.home import home, style, script

def create_page(page_name):
  with open(f'templates/{page_name if page_name else "home"}.html', 'w') as page:
    page.write(style+home+script)
    page.close()

create_page(home)
app = Flask(__name__, template_folder="templates")

@app.route("/")
def index():
    # Ensure template exists
    template_path = os.path.join(app.template_folder, "templates")
    if not os.path.exists(template_path):
        return "<h1>Home Page Not Found</h1>", 404
    return render_template("home.html")

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
