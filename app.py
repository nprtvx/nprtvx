from flask import Flask

app = Flask(__name__)


home = f"""
<div class='home' id='home'>
<h1>nprtvx</h1>
<p>welcome</p>
</div>
<style>
* {{
margin: 0;
padding: 0;
box-sizing: border-box;
}}
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
        padding: 2rem;
      }}
    </style>
    <script>
      const popeye = document.getElemenetById('popeye');
      document.append(popeye*8)
    </script>
  """

@app.route('/bring-it-on')
def bringiton():
  return popeye

@app.route('string-test')
def stringtest():
  return f"""ello {'mastaru'.upper()}! em chesthunnaru?"""
