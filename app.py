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
  url = "https://nortvx.onrender.com/ello"
  data = {"text": "ello world"}
  
  try:
    response = requests.post(url, data=data, timeout=10)
    response.raise_for_status()  # Raises HTTPError if the response was an HTTP error
  except requests.exceptions.HTTPError as errh:
    print("HTTP Error:", errh)
  except requests.exceptions.ConnectionError as errc:
    print("Error Connecting:", errc)
  except requests.exceptions.Timeout as errt:
    print("Timeout Error:", errt)
  except requests.exceptions.RequestException as err:
    print("Oops: Something Else", err)
  else:
    print("Request succeeded:", response.json())
  
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
