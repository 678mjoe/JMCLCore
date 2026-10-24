/* Synthetic benign process: no Minecraft, accounts, or network. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <signal.h>
int main(int argc,char**argv){
 if(argc<2)return 1;
 if(!strcmp(argv[1],"emit")){fwrite("out fake-",1,9,stdout);fflush(stdout);usleep(10000);fwrite("secret\r\n\n",1,9,stdout);fflush(stdout);usleep(10000);fwrite("err\000binary\r",1,11,stderr);return 7;}
 if(!strcmp(argv[1],"large")){for(int i=0;i<32769;i++)putchar('x');putchar('\n');return 0;}
 if(!strcmp(argv[1],"args")){for(int i=2;i<argc;i++)printf("%s\n",argv[i]);return 0;}
 if(!strcmp(argv[1],"env")){printf("HOME=%s\nFAKE_TOKEN=%s\nJAVA_TOOL_OPTIONS=%s\n",getenv("HOME"),getenv("FAKE_TOKEN")?"present":"absent",getenv("JAVA_TOOL_OPTIONS")?"present":"absent");return 0;}
 if(!strcmp(argv[1],"signal")){raise(SIGTERM);return 0;}
 if(!strcmp(argv[1],"descendant")){pid_t child=fork();if(child==0){usleep(600000);_exit(0);}printf("parent done\n");return 0;}
 return 2;
}
