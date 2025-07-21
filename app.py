from flask import Flask

app = Flask(__name__)

default_style = f"""
* {{
margin: 0;
padding: 0;
box-sizing: border-box;
}}
"""

home = f"""
<div class='home' id='home'>
<h1>nprtvx</h1>
<p>welcome</p>
</div>
<style>
{default_style}
.home {{
position: absolute;
top: 0;
bottom: 0;
right: 0;
left: 0;
box-shadow: 0 0 2rem inset;
display: flex;
align-items: center;
justify-content: center;
flex-direction: column;
}}
.home h1 {{
font-size: 48px;
color: #fffe;
background-color: #000d;
text-align: center;
margin: 26px 26px 0 0;
padding: 11px 26px 11px 26px;
}}
</style>
"""

@app.route('/')
def index():
  return home

popeye = f"""
<div class='popeye' id='popeye'></div>
<style>
.popeye {{
background-color: #8926;
}}
</style>
<script>
const popeye = document.getElemenetById('popeye');
</script>
"""

@app.route('/bring-it-on')
def bringiton():
  return popeye

@app.route('/string-test')
def stringtest():
  return f"""ello {'mastaru'.upper()}! em chesthunnaru?"""


import requests

@app.route('/ello')
def ello(name):
  response = requests.post('/ello/', data={'text': 'hello world'})
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
  reutrn four04
