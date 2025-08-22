home = f"""
<body>
<div class="nprtvx">
<h1 id="nxtitle">nprtvx</h1>
<div class="section-nm">

</div>
</body>
"""

style = f"""
<style>
.nprtvx {{
position: absolute;
top: 0;
bottom: 0;
left: 0;
right: 0;
background-image: linear-gradient(26deg, black, white);
}}
</style>
"""

script = f"""
<script>
const nxtitle = document.getElementById("nxtitle");
nxtitle.addEventListener("click", (event) {{
console.log(event);
location.href = "/";
}});
</script>
"""

if __name__=="__main__":
  with open('templates/home.html', 'w') as page:
    page.write(style+home+script)
    page.close()
