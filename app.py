from flask import Flask
import requests
from src.home import home
from src.popeye import popeye
from src.account.users import username

app = Flask(__name__)

@app.route('/')
def index():
  return home

@app.route('/popeye')
def bringiton():
  return popeye

@app.route('/string-test')
def stringtest():
  return f"""ello {'mastaru'.upper()}! em chesthunnaru?"""

@app.route('/ello')
def ello(name):
  response = requests.post('/ello/', json={'text': 'hello world'})
  return response.text

four04 = f"""
<div id='elem-404'>
<h1>404</h1>
<p></p>
</div>
<script>
const elem404 = document.getElementById('error-404');
elem404.classList.add('error-404');
</script>
<style>
body {{
display: flex;
align-items: center;
justify-content: center;
flex-direction: column;
}}
.error-404 {{
width: 80%;
}}
.error-404 h1 {{
font-size: 48px;
font-family: system-ui;
}}
</style>
"""

@app.errorhandler(404)
def error404():
  return four04

@app.errorhandler(502)
def error502():
  return f"""<div id='error-502'>502</div><style>#error-502{{font-size: 72px;color: #892C;}}</style>"""
