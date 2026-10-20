import sys, re
from gen import svg
def compact(P=""):
    s = svg("full", "simple", P, compact=True)
    s = re.sub(r'\s*<filter id="[^"]*"[^>]*>.*?</filter>', '', s)
    return s.replace(' width="1024" height="1024"', '', 1).replace('\n', '')
if __name__=="__main__":
    print(compact(sys.argv[1] if len(sys.argv)>1 else ""), end="")
