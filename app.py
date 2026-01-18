from flask import Flask, render_template, abort
from werkzeug.middleware.dispatcher import DispatcherMiddleware
from a2wsgi import ASGIMiddleware
import os
from src.home import home
from src.popeye import popeye

def create_page(page_name: str):
  if page_name:
    with open(f"templates/{page_name}.html", 'w') as page:
      if(page_name == "home"):
        page.write(home)
      elif page_name == "popeye":
        page.write(popeye)
      page.close()
  template_path = os.path.join(app.template_folder, f"{page_name}.html")
  if not os.path.exists(template_path):
      return f"<h1>{page_name.capitalize()}: Page Not Found</h1>", 404
  return template_path.split('/')[1]

app = Flask(__name__, template_folder="templates")

@app.route("/")
def index():
  # Ensure template exists
  return render_template(create_page("home"))

@app.route("/popeye")
def pope():
  # Ensure template exists
  return render_template(create_page("popeye"))

if __name__ == "__main__":
    app.run()
