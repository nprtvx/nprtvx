import requests

username = 'popeye'
url = f'https:////nprtvx.onrender.com//account//users/{username}'

response = requests.get(url)
print(response)

postres = response.post(url, data={'body': """
        welcome popeye
        how's it going
        
        
        
        
        
        do something!!!
        cya!
"""}) if response.status == '200' else ""

print(postres)



#eof